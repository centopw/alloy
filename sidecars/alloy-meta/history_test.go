package main

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/rs/zerolog"
	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/methods"
	"go.mau.fi/mautrix-meta/pkg/messagix/securehistory"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
	"go.mau.fi/whatsmeow/proto/waArmadilloBackupCommon"
	"go.mau.fi/whatsmeow/proto/waArmadilloBackupMessage"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waConsumerApplication"
	"go.mau.fi/whatsmeow/proto/waMediaTransport"
	"go.mau.fi/whatsmeow/proto/waMsgApplication"
	"go.mau.fi/whatsmeow/store"
	waTypes "go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/util/keys"
	"google.golang.org/protobuf/proto"
)

// fakeHistoryTransport stubs Meta's backup service. It counts the one write enrolment performs so
// a test can prove nothing was registered, and it records the PIN-derived input indirectly by
// never seeing the PIN: RecoverPIN gets it, and the test asserts it reaches no log or blob.
type fakeHistoryTransport struct {
	backup     *securehistory.Backup
	backupErr  error
	recovery   *securehistory.Recovery
	recoverErr error
	virtual    *securehistory.VirtualDevice
	device     *securehistory.Device
	addErr     error
	addCalls   int
	page       *securehistory.MessageRange
	fetchErr   error
	fetchCalls int
}

func (f *fakeHistoryTransport) RegistrationID() string { return "registration-1" }

func (f *fakeHistoryTransport) GetBackup(context.Context) (*securehistory.Backup, error) {
	return f.backup, f.backupErr
}

func (f *fakeHistoryTransport) RecoverPIN(_ context.Context, _ *securehistory.Backup, pin []byte) (*securehistory.Recovery, error) {
	if f.recoverErr != nil {
		return nil, f.recoverErr
	}
	if f.recovery == nil {
		return &securehistory.Recovery{Code: bytes.Repeat([]byte{1}, 40)}, nil
	}
	return f.recovery, nil
}

func (f *fakeHistoryTransport) GetVirtualDevice(context.Context, *securehistory.Backup, int64, []byte) (*securehistory.VirtualDevice, error) {
	if f.virtual == nil {
		return &securehistory.VirtualDevice{}, nil
	}
	return f.virtual, nil
}

func (f *fakeHistoryTransport) AddDevice(_ context.Context, enrollment *securehistory.Enrollment) (*securehistory.Device, error) {
	f.addCalls++
	if f.addErr != nil {
		return nil, f.addErr
	}
	if f.device != nil {
		return f.device, nil
	}
	device := *enrollment.Device
	device.ID = "device-1"
	return &device, nil
}

func (f *fakeHistoryTransport) FetchMessages(context.Context, *securehistory.Device, string, bool, int64, int) (*securehistory.MessageRange, error) {
	f.fetchCalls++
	return f.page, f.fetchErr
}

// fakeHistoryKeys stands in for the library's local crypto so enrolment can be exercised without
// real ciphertext.
type fakeHistoryKeys struct{}

func (fakeHistoryKeys) DeriveEpochs(*securehistory.VirtualDevice) (map[string]securehistory.Epoch, error) {
	return map[string]securehistory.Epoch{"epoch-1": {ID: "epoch-1", AnonID: []byte("1"), RootKey: bytes.Repeat([]byte{2}, 32)}}, nil
}

func (fakeHistoryKeys) PrepareDevice(_ *securehistory.VirtualDevice, backupID, registrationID string, _ *keys.KeyPair, epochs map[string]securehistory.Epoch) (*securehistory.Enrollment, error) {
	return &securehistory.Enrollment{
		Device: &securehistory.Device{BackupID: backupID, Epochs: epochs},
		Info:   &securehistory.DeviceInfo{BackupID: backupID, DeviceRegistrationID: registrationID},
	}, nil
}

// testMessenger assembles a logged-in connector wired to stubs, with a log the test can inspect.
func testMessenger(t *testing.T, transport *fakeHistoryTransport) (*Messenger, *bytes.Buffer, *bytes.Buffer) {
	t.Helper()
	var logBuf bytes.Buffer
	var out bytes.Buffer
	m := newMessenger(zerolog.New(&logBuf).Level(zerolog.DebugLevel), newWriter(&out), defaultSettings())
	c := &cookies.Cookies{Platform: types.Messenger}
	c.UpdateValues(map[cookies.MetaCookieName]string{
		cookies.FBCookieXS:     "token",
		cookies.FBCookieCUser:  "100012345",
		cookies.MetaCookieDatr: "datr-value",
	})
	m.cookies = c
	m.cli = &messagix.Client{}
	m.ownID = 100012345
	m.remoteUser = "100012345"
	m.device = &store.Device{IdentityKey: keys.NewKeyPair()}
	m.historyClient = transport
	m.historyKeyDeriver = fakeHistoryKeys{}
	return m, &logBuf, &out
}

// sessionFrames decodes the SessionUpdated blobs written to stdout.
func sessionFrames(t *testing.T, out *bytes.Buffer) []string {
	t.Helper()
	var blobs []string
	for _, line := range strings.Split(strings.TrimSpace(out.String()), "\n") {
		if line == "" {
			continue
		}
		var frame EventFrame
		if err := json.Unmarshal([]byte(line), &frame); err != nil {
			t.Fatalf("event frame is not JSON: %v (%s)", err, line)
		}
		if frame.Type != "event" {
			continue
		}
		raw, err := json.Marshal(frame.Event)
		if err != nil {
			t.Fatalf("event is not JSON: %v", err)
		}
		var typed struct {
			Type    string `json:"type"`
			Session string `json:"session"`
		}
		if err := json.Unmarshal(raw, &typed); err != nil {
			continue
		}
		if typed.Type == "session_updated" {
			blobs = append(blobs, typed.Session)
		}
	}
	return blobs
}

func pinPin() string { return "123456" }

// TestEnrollmentPersistsOnlyAfterAddDevice pins the one-time nature of enrolment: a successful
// AddDevice stores the device and publishes the session once; a rejected AddDevice stores nothing.
func TestEnrollmentPersistsOnlyAfterAddDevice(t *testing.T) {
	transport := &fakeHistoryTransport{backup: &securehistory.Backup{ID: "backup-1"}}
	m, _, out := testMessenger(t, transport)

	result, werr := m.historyPin(context.Background(), json.RawMessage(`{"pin":"123456"}`))
	if werr != nil {
		t.Fatalf("historyPin failed: %s", werr)
	}
	status, ok := result.(*HistoryStatus)
	if !ok || !status.Enrolled || !status.PinBackup {
		t.Fatalf("historyPin result = %#v, want enrolled", result)
	}
	if transport.addCalls != 1 {
		t.Fatalf("AddDevice calls = %d, want 1", transport.addCalls)
	}
	if m.historyDev == nil || m.historyDev.ID != "device-1" {
		t.Fatalf("device not persisted: %+v", m.historyDev)
	}
	blobs := sessionFrames(t, out)
	if len(blobs) != 1 {
		t.Fatalf("session_updated count = %d, want 1", len(blobs))
	}
	raw, err := base64.StdEncoding.DecodeString(blobs[0])
	if err != nil {
		t.Fatalf("session blob is not base64: %v", err)
	}
	var stored sessionData
	if err := json.Unmarshal(raw, &stored); err != nil {
		t.Fatalf("session blob is not JSON: %v", err)
	}
	if stored.History == nil || stored.History.ID != "device-1" {
		t.Fatalf("session did not carry the enrolled device: %+v", stored.History)
	}

	// A second request must not register again.
	if _, werr := m.historyPin(context.Background(), json.RawMessage(`{"pin":"123456"}`)); werr != nil {
		t.Fatalf("second historyPin failed: %s", werr)
	}
	if transport.addCalls != 1 {
		t.Fatalf("AddDevice calls after second request = %d, want 1", transport.addCalls)
	}
}

func TestEnrollmentPersistsNothingWhenAddDeviceFails(t *testing.T) {
	transport := &fakeHistoryTransport{
		backup: &securehistory.Backup{ID: "backup-1"},
		addErr: &securehistory.EnrollmentError{Code: "rejected"},
	}
	m, _, out := testMessenger(t, transport)

	_, werr := m.historyPin(context.Background(), json.RawMessage(`{"pin":"123456"}`))
	if werr == nil {
		t.Fatal("historyPin should fail when AddDevice fails")
	}
	if m.historyDev != nil {
		t.Fatalf("a failed enrolment persisted a device: %+v", m.historyDev)
	}
	if blobs := sessionFrames(t, out); len(blobs) != 0 {
		t.Fatalf("a failed enrolment published a session: %d blobs", len(blobs))
	}
}

// TestHistoryPinFailureCodes keeps the three stable codes and proves none of them logs at warn.
func TestHistoryPinFailureCodes(t *testing.T) {
	attempts := 2
	cases := []struct {
		name         string
		transport    *fakeHistoryTransport
		wantType     string
		wantAttempts *int
	}{
		{
			name:      "no backup",
			transport: &fakeHistoryTransport{backupErr: securehistory.ErrNoPINBackup},
			wantType:  "pin_backup_unavailable",
		},
		{
			name: "wrong pin",
			transport: &fakeHistoryTransport{
				backup:     &securehistory.Backup{ID: "backup-1"},
				recoverErr: &securehistory.RecoveryError{Code: "wrong_pin", AttemptsRemaining: &attempts},
			},
			wantType:     "pin_rejected",
			wantAttempts: &attempts,
		},
		{
			name: "locked out",
			transport: &fakeHistoryTransport{
				backup:     &securehistory.Backup{ID: "backup-1"},
				recoverErr: &securehistory.RecoveryError{Code: "locked", AttemptsRemaining: new(0)},
			},
			wantType:     "pin_locked_out",
			wantAttempts: new(0),
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			m, logBuf, out := testMessenger(t, tc.transport)
			_, werr := m.historyPin(context.Background(), json.RawMessage(`{"pin":"123456"}`))
			if werr == nil || werr.Type != tc.wantType {
				t.Fatalf("error = %#v, want type %q", werr, tc.wantType)
			}
			if tc.wantAttempts != nil {
				if werr.AttemptsRemaining == nil || *werr.AttemptsRemaining != *tc.wantAttempts {
					t.Fatalf("attempts_remaining = %v, want %d", werr.AttemptsRemaining, *tc.wantAttempts)
				}
				raw, err := json.Marshal(werr)
				if err != nil {
					t.Fatalf("error does not encode: %v", err)
				}
				if !strings.Contains(string(raw), `"attempts_remaining":`) {
					t.Fatalf("attempts_remaining missing from the wire shape: %s", raw)
				}
			}
			if strings.Contains(logBuf.String(), `"level":"warn"`) {
				t.Fatalf("a normal PIN failure logged at warn:\n%s", logBuf.String())
			}
			if blobs := sessionFrames(t, out); len(blobs) != 0 {
				t.Fatalf("a PIN failure published a session: %d blobs", len(blobs))
			}
		})
	}
}

// TestPinNeverLeaks asserts the PIN appears in no log line and in no persisted blob.
func TestPinNeverLeaks(t *testing.T) {
	transport := &fakeHistoryTransport{
		backup:     &securehistory.Backup{ID: "backup-1"},
		recoverErr: &securehistory.RecoveryError{Code: "wrong_pin", AttemptsRemaining: new(3)},
	}
	m, logBuf, out := testMessenger(t, transport)
	if _, werr := m.historyPin(context.Background(), json.RawMessage(`{"pin":"975310"}`)); werr == nil {
		t.Fatal("expected the PIN to be rejected")
	}
	if strings.Contains(logBuf.String(), "975310") {
		t.Fatalf("the PIN reached the log:\n%s", logBuf.String())
	}
	for _, blob := range sessionFrames(t, out) {
		raw, err := base64.StdEncoding.DecodeString(blob)
		if err != nil {
			t.Fatalf("session blob is not base64: %v", err)
		}
		if strings.Contains(string(raw), "975310") {
			t.Fatalf("the PIN reached the session blob: %s", raw)
		}
	}

	// And with a successful enrolment, which is the path that does publish the session.
	transport = &fakeHistoryTransport{backup: &securehistory.Backup{ID: "backup-1"}}
	m, logBuf, out = testMessenger(t, transport)
	if _, werr := m.historyPin(context.Background(), json.RawMessage(`{"pin":"975310"}`)); werr != nil {
		t.Fatalf("historyPin failed: %s", werr)
	}
	if strings.Contains(logBuf.String(), "975310") {
		t.Fatalf("the PIN reached the log:\n%s", logBuf.String())
	}
	for _, blob := range sessionFrames(t, out) {
		raw, err := base64.StdEncoding.DecodeString(blob)
		if err != nil {
			t.Fatalf("session blob is not base64: %v", err)
		}
		if strings.Contains(string(raw), "975310") {
			t.Fatalf("the PIN reached the session blob: %s", raw)
		}
	}
}

// TestDecryptDegradesPerMessage proves one unsupported message does not cost the page: the library
// fails the whole page, the wrapper isolates the message and returns the rest.
func TestDecryptDegradesPerMessage(t *testing.T) {
	good := unencryptedArchiveMessage(t, "mid.good", "200", 1_700_000_000_000, textContent("hello archive"))
	bad := &securehistory.ArchiveMessage{
		ID: "999",
		Protobufs: &securehistory.ArchiveProtobufs{
			Message: &securehistory.ArchiveProtobuf{
				EncryptionVersion: "ENCRYPTION_FROM_THE_FUTURE",
				EpochID:           "epoch-1",
				EpochAnonID:       "1",
				Encrypted:         bytes.Repeat([]byte{4}, 64),
			},
		},
	}
	page := &securehistory.MessageRange{Messages: []*securehistory.ArchiveMessage{good, bad}}
	device := &securehistory.Device{
		ID: "device-1",
		Epochs: map[string]securehistory.Epoch{
			"epoch-1": {ID: "epoch-1", AnonID: []byte("1"), RootKey: bytes.Repeat([]byte{2}, 32)},
		},
	}
	if _, err := page.Decrypt(device, "200", 100, false); err == nil {
		t.Fatal("the library is expected to fail the whole page")
	}

	m, _, _ := testMessenger(t, &fakeHistoryTransport{})
	decrypted := m.decryptArchivePage(page, device, "200", 100, false)
	if len(decrypted) != 1 {
		t.Fatalf("decrypted messages = %d, want 1", len(decrypted))
	}
	rows := m.archiveRows(decrypted, "e2ee:fbid:200@msgr", waTypesJID("200"))
	if len(rows) != 1 || rows[0].RemoteMsgID != "mid.good" {
		t.Fatalf("rows = %#v, want only the good message", rows)
	}
	if rows[0].Body == nil || *rows[0].Body != "hello archive" {
		t.Fatalf("body = %v, want hello archive", rows[0].Body)
	}
}

// TestArchiveMappingMatchesLiveShape covers the field mapping: metadata id, timestamp, own flag,
// reply, edit override, reaction and the archive attachment URL matched by object id.
func TestArchiveMappingMatchesLiveShape(t *testing.T) {
	const objectID = "object-1"
	transportRaw, err := imageTransport(objectID, "image/jpeg", 1280, 720)
	if err != nil {
		t.Fatalf("build image transport: %v", err)
	}
	var image waConsumerApplication.ConsumerApplication_ImageMessage
	if err := image.Set(transportRaw); err != nil {
		t.Fatalf("set image transport: %v", err)
	}
	caption := "look"
	image.Caption = &waCommon.MessageText{Text: &caption}

	parent := archivedApplication(t, "mid.parent", "200", 1_700_000_000_000, &waConsumerApplication.ConsumerApplication_Content{
		Content: &waConsumerApplication.ConsumerApplication_Content_ImageMessage{ImageMessage: &image},
	})
	parent.GetMetadata().TimestampMS = nil // the stanza timestamp is authoritative

	editBody := "look (edited)"
	edit := archivedApplication(t, "", "200", 1_700_000_000_500, &waConsumerApplication.ConsumerApplication_Content{
		Content: &waConsumerApplication.ConsumerApplication_Content_EditMessage{
			EditMessage: &waConsumerApplication.ConsumerApplication_EditMessage{
				Key:     &waCommon.MessageKey{ID: proto.String("mid.parent")},
				Message: &waCommon.MessageText{Text: &editBody},
			},
		},
	})
	reaction := archivedApplication(t, "", "300", 1_700_000_000_600, &waConsumerApplication.ConsumerApplication_Content{
		Content: &waConsumerApplication.ConsumerApplication_Content_ReactionMessage{
			ReactionMessage: &waConsumerApplication.ConsumerApplication_ReactionMessage{
				Key:  &waCommon.MessageKey{ID: proto.String("mid.parent")},
				Text: proto.String("\U0001F44D"),
			},
		},
	})

	message := &securehistory.DecryptedMessage{
		ID:      "archive-otid-1",
		Message: &securehistory.DecryptedProtobuf{Message: parent, TimestampMS: 1_700_000_000_000},
		Supplementals: map[string]*securehistory.DecryptedProtobuf{
			"edit:mid.parent": {Message: edit, TimestampMS: 1_700_000_000_500},
			"reaction:300":    {Message: reaction, TimestampMS: 1_700_000_000_600},
		},
		Attachments: []securehistory.ArchiveAttachment{{URL: "https://cdn.example/photo.jpg", ObjectID: objectID}},
	}

	m, _, out := testMessenger(t, &fakeHistoryTransport{})
	rows := m.archiveRows([]*securehistory.DecryptedMessage{message}, "e2ee:fbid:200@msgr", waTypesJID("200"))
	if len(rows) != 1 {
		t.Fatalf("rows = %d, want 1", len(rows))
	}
	row := rows[0]
	if row.RemoteMsgID != "mid.parent" {
		t.Errorf("remote_msg_id = %q, want the metadata message id", row.RemoteMsgID)
	}
	if row.SenderRemoteID != "200" || row.IsOwn {
		t.Errorf("sender = %q own = %v", row.SenderRemoteID, row.IsOwn)
	}
	if row.SentAt != 1_700_000_000_000 {
		t.Errorf("sent_at = %d", row.SentAt)
	}
	if row.Kind != "image" {
		t.Errorf("kind = %q, want image", row.Kind)
	}
	if row.Body == nil || *row.Body != editBody {
		t.Errorf("body = %v, want the edit override %q", row.Body, editBody)
	}
	if row.EditedAt == nil || *row.EditedAt != 1_700_000_000_500 {
		t.Errorf("edited_at = %v", row.EditedAt)
	}
	if row.Media == nil || row.Media.RemoteRef != "https://cdn.example/photo.jpg" {
		t.Errorf("media = %#v, want the archive attachment URL", row.Media)
	}
	if row.Media != nil && (row.Media.Mime != "image/jpeg" || row.Media.Width == nil || *row.Media.Width != 1280) {
		t.Errorf("media transport fields lost: %#v", row.Media)
	}

	var sawReaction bool
	for _, line := range strings.Split(strings.TrimSpace(out.String()), "\n") {
		if !strings.Contains(line, `"reaction"`) {
			continue
		}
		sawReaction = true
		if !strings.Contains(line, `"sender_remote_id":"300"`) || !strings.Contains(line, "mid.parent") {
			t.Errorf("reaction event wrong: %s", line)
		}
	}
	if !sawReaction {
		t.Errorf("no reaction event was emitted for the archive supplemental")
	}
}

// TestArchiveCursorRoundTrips pins that the cursor is a message id whose timestamp is the archive's
// next page boundary, which is what the next backfill parses.
func TestArchiveCursorRoundTrips(t *testing.T) {
	const ts = int64(1_700_000_123_456)
	cursor := archiveCursorID(ts)
	parsed, err := methodsParse(cursor)
	if err != nil {
		t.Fatalf("cursor does not parse as a message id: %v", err)
	}
	if parsed != ts {
		t.Fatalf("parsed cursor timestamp = %d, want %d", parsed, ts)
	}
}

func TestHistoryUnavailableIsNotAnError(t *testing.T) {
	page := &securehistory.MessageRange{}
	for _, err := range []error{
		errors.New("archive thread was not found"),
		errors.New("archive is no longer available"),
		errors.New("archive response belongs to a different backup"),
	} {
		result := archiveFetchFailure(err)
		pageResult, ok := result.(MessagePage)
		if !ok {
			t.Fatalf("%v became a protocol error instead of an empty page", err)
		}
		if len(pageResult.Items) != 0 || pageResult.NextCursor != nil {
			t.Fatalf("unavailable history shaped wrongly: %#v", pageResult)
		}
	}
	_ = page
}

// --- proto fixtures ---

func textContent(text string) *waConsumerApplication.ConsumerApplication_Content {
	return &waConsumerApplication.ConsumerApplication_Content{
		Content: &waConsumerApplication.ConsumerApplication_Content_MessageText{
			MessageText: &waCommon.MessageText{Text: proto.String(text)},
		},
	}
}

// archivedApplication builds the plaintext a backup message carries: a consumer application
// wrapped in a message application, exactly as the live socket decodes it.
func archivedApplication(t *testing.T, id, sender string, ts int64, content *waConsumerApplication.ConsumerApplication_Content) *waArmadilloBackupMessage.BackupMessage {
	t.Helper()
	consumer := &waConsumerApplication.ConsumerApplication{
		Payload: &waConsumerApplication.ConsumerApplication_Payload{
			Payload: &waConsumerApplication.ConsumerApplication_Payload_Content{Content: content},
		},
	}
	var sub waMsgApplication.MessageApplication_SubProtocolPayload_ConsumerMessage
	if err := sub.Set(consumer); err != nil {
		t.Fatalf("set consumer application: %v", err)
	}
	application := &waMsgApplication.MessageApplication{
		Payload: &waMsgApplication.MessageApplication_Payload{
			Content: &waMsgApplication.MessageApplication_Payload_SubProtocol{
				SubProtocol: &waMsgApplication.MessageApplication_SubProtocolPayload{
					SubProtocol: &sub,
				},
			},
		},
		Metadata: &waMsgApplication.MessageApplication_Metadata{
			QuotedMessage: &waMsgApplication.MessageApplication_Metadata_QuotedMessage{
				StanzaID: proto.String("mid.quoted"),
			},
		},
	}
	raw, err := proto.Marshal(application)
	if err != nil {
		t.Fatalf("marshal application: %v", err)
	}
	return &waArmadilloBackupMessage.BackupMessage{
		Metadata: &waArmadilloBackupCommon.Metadata{
			MessageID:   proto.String(id),
			SenderID:    proto.String(sender),
			TimestampMS: proto.Int64(ts),
		},
		Payload: &waArmadilloBackupMessage.BackupMessage_EncryptedTransportMessage{
			EncryptedTransportMessage: raw,
		},
	}
}

// unencryptedArchiveMessage wraps a fixture in the library's plaintext stanza, which needs no key.
func unencryptedArchiveMessage(t *testing.T, id, sender string, ts int64, content *waConsumerApplication.ConsumerApplication_Content) *securehistory.ArchiveMessage {
	t.Helper()
	message := archivedApplication(t, id, sender, ts, content)
	raw, err := proto.Marshal(message)
	if err != nil {
		t.Fatalf("marshal backup message: %v", err)
	}
	return &securehistory.ArchiveMessage{
		ID: "otid-" + id,
		Protobufs: &securehistory.ArchiveProtobufs{
			Unencrypted: &struct {
				Message     []byte `json:"unencrypted_protobuf"`
				TimestampMS int64  `json:"protobuf_timestamp_ms,string"`
			}{Message: raw, TimestampMS: ts},
		},
	}
}

func imageTransport(objectID, mime string, width, height uint32) (*waMediaTransport.ImageTransport, error) {
	transport := &waMediaTransport.ImageTransport{
		Integral: &waMediaTransport.ImageTransport_Integral{
			Transport: &waMediaTransport.WAMediaTransport{
				Integral: &waMediaTransport.WAMediaTransport_Integral{
					FileSHA256:    bytes.Repeat([]byte{7}, 32),
					MediaKey:      bytes.Repeat([]byte{8}, 32),
					FileEncSHA256: bytes.Repeat([]byte{9}, 32),
					DirectPath:    proto.String("/v/t62.7118-24/photo.enc"),
				},
				Ancillary: &waMediaTransport.WAMediaTransport_Ancillary{
					ObjectID:   proto.String(objectID),
					Mimetype:   proto.String(mime),
					FileLength: proto.Uint64(4096),
				},
			},
		},
		Ancillary: &waMediaTransport.ImageTransport_Ancillary{
			Width:  proto.Uint32(width),
			Height: proto.Uint32(height),
		},
	}
	return transport, nil
}

func waTypesJID(user string) (jid waTypes.JID) {
	jid.User = user
	jid.Server = waTypes.MessengerServer
	return jid
}

func methodsParse(id string) (int64, error) {
	return methods.ParseMessageID(id)
}
