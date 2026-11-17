package main

import (
	"context"
	"encoding/json"
	"errors"
	"regexp"
	"strconv"
	"strings"
	"time"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/methods"
	"go.mau.fi/mautrix-meta/pkg/messagix/securehistory"
	waTypes "go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/util/keys"
)

// historyTransport is the network half of Meta's encrypted archive: everything the library talks
// to the backup service for. It is an interface so enrolment and paging can be tested against a
// stub; production always wraps securehistory.Client, and no crypto is implemented here.
type historyTransport interface {
	RegistrationID() string
	GetBackup(ctx context.Context) (*securehistory.Backup, error)
	RecoverPIN(ctx context.Context, backup *securehistory.Backup, pin []byte) (*securehistory.Recovery, error)
	GetVirtualDevice(ctx context.Context, backup *securehistory.Backup, userID int64, code []byte) (*securehistory.VirtualDevice, error)
	AddDevice(ctx context.Context, enrollment *securehistory.Enrollment) (*securehistory.Device, error)
	FetchMessages(ctx context.Context, device *securehistory.Device, threadID string, before bool, timestampMS int64, count int) (*securehistory.MessageRange, error)
}

// historyKeys is the local half of enrolment: epoch derivation and device preparation. Both are
// the library's own functions; this seam exists only so tests can enrol without real ciphertext.
type historyKeys interface {
	DeriveEpochs(device *securehistory.VirtualDevice) (map[string]securehistory.Epoch, error)
	PrepareDevice(device *securehistory.VirtualDevice, backupID, registrationID string, identity *keys.KeyPair, epochs map[string]securehistory.Epoch) (*securehistory.Enrollment, error)
}

// secureHistoryTransport adapts securehistory.Client for one logged-in messagix session.
type secureHistoryTransport struct {
	client *securehistory.Client
}

func (t secureHistoryTransport) RegistrationID() string { return t.client.RegistrationID }

func (t secureHistoryTransport) GetBackup(ctx context.Context) (*securehistory.Backup, error) {
	return t.client.GetBackup(ctx)
}

func (t secureHistoryTransport) RecoverPIN(ctx context.Context, backup *securehistory.Backup, pin []byte) (*securehistory.Recovery, error) {
	return t.client.RecoverPIN(ctx, backup, pin)
}

func (t secureHistoryTransport) GetVirtualDevice(ctx context.Context, backup *securehistory.Backup, userID int64, code []byte) (*securehistory.VirtualDevice, error) {
	return t.client.GetVirtualDevice(ctx, backup, userID, code)
}

func (t secureHistoryTransport) AddDevice(ctx context.Context, enrollment *securehistory.Enrollment) (*securehistory.Device, error) {
	return t.client.AddDevice(ctx, enrollment)
}

func (t secureHistoryTransport) FetchMessages(ctx context.Context, device *securehistory.Device, threadID string, before bool, timestampMS int64, count int) (*securehistory.MessageRange, error) {
	return t.client.FetchMessages(ctx, device, threadID, before, timestampMS, count)
}

// secureHistoryKeys defers to the library's own implementations.
type secureHistoryKeys struct{}

func (secureHistoryKeys) DeriveEpochs(device *securehistory.VirtualDevice) (map[string]securehistory.Epoch, error) {
	return device.DeriveEpochs()
}

func (secureHistoryKeys) PrepareDevice(device *securehistory.VirtualDevice, backupID, registrationID string, identity *keys.KeyPair, epochs map[string]securehistory.Epoch) (*securehistory.Enrollment, error) {
	return device.PrepareDevice(backupID, registrationID, identity, epochs)
}

// historyPinPattern is the only PIN shape Meta accepts: exactly six decimal digits.
var historyPinPattern = regexp.MustCompile(`^[0-9]{6}$`)

// archiveTransport returns the transport configured for this Messenger. Tests inject one; otherwise
// it wraps the live messagix client, which must be authenticated.
func (m *Messenger) archiveTransport(cli *messagix.Client) historyTransport {
	if m.historyClient != nil {
		return m.historyClient
	}
	if cli == nil {
		return nil
	}
	return secureHistoryTransport{client: cli.GetSecureHistoryClient()}
}

// archiveKeys returns the key-derivation seam, real unless a test replaced it.
func (m *Messenger) archiveKeys() historyKeys {
	if m.historyKeyDeriver != nil {
		return m.historyKeyDeriver
	}
	return secureHistoryKeys{}
}

// historySnapshot returns the enrolled archive device, if any. historyMu guards it because a page
// that derives new epochs replaces the whole pointer.
func (m *Messenger) historySnapshot() *securehistory.Device {
	m.historyMu.Lock()
	defer m.historyMu.Unlock()
	return m.historyDev
}

// encodeCurrentSession re-encodes the live session, archive device included. The host persists the
// blob it receives, so this is also how a newly enrolled or newly extended device is saved.
func (m *Messenger) encodeCurrentSession() (string, error) {
	m.mu.Lock()
	c, native := m.cookies, m.native
	m.mu.Unlock()
	if c == nil {
		return "", errors.New("no session to encode")
	}
	return encodeSession(c, native, m.historySnapshot())
}

// publishSession tells the host to persist the session. Called after enrolment and after any page
// whose epoch edges extended the device.
func (m *Messenger) publishSession() {
	blob, err := m.encodeCurrentSession()
	if err != nil {
		m.log.Debug().Err(err).Msg("Not publishing the archive session")
		return
	}
	m.emit(NewSessionUpdatedEvent(blob))
}

// historyStatus reports the archive capability for the login payload. A missing PIN backup is a
// capability, not a failure, and is only ever logged at debug.
func (m *Messenger) historyStatus(ctx context.Context, cli *messagix.Client, enrolled bool) *HistoryStatus {
	status := &HistoryStatus{Enrolled: enrolled}
	transport := m.archiveTransport(cli)
	if transport == nil {
		return status
	}
	backup, err := transport.GetBackup(ctx)
	switch {
	case err == nil && backup != nil && backup.ID != "":
		status.PinBackup = true
	case errors.Is(err, securehistory.ErrNoPINBackup):
		m.log.Debug().Msg("The account has no Messenger PIN backup")
	default:
		m.log.Debug().Err(err).Msg("Could not read the Messenger PIN backup capability")
	}
	return status
}

// enrollArchive runs the one-time enrolment handshake: recover the backup key from the PIN, derive
// the epoch map, prepare a device with the account's WhatsApp identity, then register it with
// AddDevice. Nothing is persisted before AddDevice succeeds: the library evolves the one-time OCMF
// scalar during preparation, and re-preparing from a stale blob would replay a rotation token.
func enrollArchive(ctx context.Context, transport historyTransport, keyDeriver historyKeys, pin string, ownID int64, identity *keys.KeyPair) (*securehistory.Device, error) {
	backup, err := transport.GetBackup(ctx)
	if err != nil {
		return nil, err
	}
	recovery, err := transport.RecoverPIN(ctx, backup, []byte(pin))
	if err != nil {
		return nil, err
	}
	virtual, err := transport.GetVirtualDevice(ctx, backup, ownID, recovery.Code)
	if err != nil {
		return nil, err
	}
	epochs, err := keyDeriver.DeriveEpochs(virtual)
	if err != nil {
		return nil, err
	}
	enrollment, err := keyDeriver.PrepareDevice(virtual, backup.ID, transport.RegistrationID(), identity, epochs)
	if err != nil {
		return nil, err
	}
	return transport.AddDevice(ctx, enrollment)
}

// historyPin handles the historyPin request: the host collected a PIN from the person.
func (m *Messenger) historyPin(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p HistoryPinParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("history_pin params: " + err.Error())
	}
	if !historyPinPattern.MatchString(p.Pin) {
		return nil, errInvalidInput("the secure storage PIN is six digits")
	}

	m.mu.Lock()
	cli := m.cli
	loggedOut := m.loggedOut
	ownID := m.ownID
	m.mu.Unlock()
	if cli == nil || loggedOut {
		return nil, errUnauthorized()
	}
	m.e2eeMu.Lock()
	identity := m.device
	m.e2eeMu.Unlock()
	if identity == nil || identity.IdentityKey == nil {
		// Enrolment signs the new device with the WhatsApp identity, so it cannot run before the
		// E2EE client exists. The host can retry once the account has connected.
		return nil, errNetwork("encrypted messaging is not ready")
	}

	transport := m.archiveTransport(cli)
	if transport == nil {
		return nil, errNetwork("no authenticated session for archive enrolment")
	}

	m.historyMu.Lock()
	if m.historyDev != nil {
		m.historyMu.Unlock()
		return &HistoryStatus{PinBackup: true, Enrolled: true}, nil
	}
	m.historyMu.Unlock()

	device, err := enrollArchive(ctx, transport, m.archiveKeys(), p.Pin, ownID, identity.IdentityKey)
	if err != nil {
		return nil, historyPinError(err)
	}

	m.historyMu.Lock()
	m.historyDev = device
	m.historyMu.Unlock()
	m.publishSession()
	return &HistoryStatus{PinBackup: true, Enrolled: true}, nil
}

// historyPinError maps the library's PIN failures onto the stable wire codes. The error text never
// contains the PIN, and none of these paths log above debug.
func historyPinError(err error) *WireError {
	var recovery *securehistory.RecoveryError
	switch {
	case errors.Is(err, securehistory.ErrNoPINBackup):
		return &WireError{Type: errHistoryNoBackup}
	case errors.As(err, &recovery):
		if recovery.AttemptsRemaining != nil && *recovery.AttemptsRemaining <= 0 {
			return errHistoryPinLockedOut()
		}
		return errHistoryPinRejected(recovery.AttemptsRemaining)
	default:
		return errProtocol(err.Error())
	}
}

// backfillArchive pages an encrypted chat backwards through Meta's encrypted archive. The archive
// ranges by timestamp, not by Alloy's message-id cursor, so Before is parsed back to the timestamp
// its message id encodes (the same encoding the live backfill uses) and the returned cursor is a
// message id carrying the archive's next timestamp.
func (m *Messenger) backfillArchive(ctx context.Context, params BackfillParams, jid waTypes.JID, limit int) (any, *WireError) {
	m.mu.Lock()
	cli := m.cli
	loggedOut := m.loggedOut
	ownID := m.ownID
	chatID := "e2ee:" + jid.String()
	group := jid.Server == waTypes.GroupServer
	if thread := m.e2eeThreads[chatID]; thread != nil {
		chatID = thread.chatID()
		group = !thread.Type.IsOneToOne()
	} else if thread := m.threads[int64(jid.UserInt())]; thread != nil && thread.chatID() == chatID {
		group = !thread.Type.IsOneToOne()
	}
	m.mu.Unlock()
	if cli == nil || loggedOut {
		return nil, errUnauthorized()
	}

	transport := m.archiveTransport(cli)
	if transport == nil {
		return nil, errNotSupported("backfill")
	}

	refTS, werr := m.archiveReference(params.Before)
	if werr != nil {
		return nil, werr
	}

	threadID := strconv.FormatUint(jid.UserInt(), 10)
	m.historyMu.Lock()
	device := m.historyDev
	if device == nil {
		m.historyMu.Unlock()
		// Not enrolled: preserve the connector's existing answer for E2EE scroll-back.
		return nil, errNotSupported("backfill")
	}
	page, err := transport.FetchMessages(ctx, device, threadID, true, refTS, limit)
	if err != nil {
		m.historyMu.Unlock()
		return archiveFetchFailure(err), nil
	}
	extended := false
	if updated, epochErr := device.WithEpochs(page.Epochs); epochErr != nil {
		m.log.Debug().Err(epochErr).Msg("Could not derive the archive page's new epochs")
	} else if updated != nil && len(updated.Epochs) != len(device.Epochs) {
		m.historyDev = updated
		device = updated
		extended = true
	}
	m.historyMu.Unlock()
	// The new epoch edges are durable state: publish before decrypting so a page whose messages
	// still fail to open does not lose them.
	if extended {
		m.publishSession()
	}

	decrypted := m.decryptArchivePage(page, device, threadID, ownID, group)
	items := m.archiveRows(decrypted, chatID, jid)
	if len(items) > limit {
		items = items[:limit]
	}

	var next *string
	if page.Range != nil && page.Range.HasMoreBefore != nil && *page.Range.HasMoreBefore &&
		page.Range.Before > 0 && page.Range.Before < refTS {
		cursor := archiveCursorID(page.Range.Before)
		next = &cursor
	}
	if items == nil {
		items = []RemoteMessage{}
	}
	return MessagePage{Items: items, NextCursor: next}, nil
}

// archiveReference resolves the page anchor. The first page starts just in the future, as
// upstream does, so the newest archived message is included.
func (m *Messenger) archiveReference(before *string) (int64, *WireError) {
	if before == nil {
		return time.Now().Add(time.Minute).UnixMilli(), nil
	}
	if ts, err := methods.ParseMessageID(*before); err == nil {
		return ts, nil
	}
	var ts int64
	if err := m.fillReferenceFromIndex(*before, &ts); err != nil {
		return 0, errInvalidInput("unknown message id " + strconv.Quote(*before))
	}
	return ts, nil
}

// archiveCursorID renders an archive timestamp as a message id, which is the cursor shape Alloy
// already understands. The archive only consumes the timestamp encoded in it.
func archiveCursorID(ts int64) string {
	return (&methods.MetaMessageID{ChatType: 'c', Time: time.UnixMilli(ts)}).String()
}

// archiveFetchFailure keeps unavailable history honest. A missing thread, a mailbox that is gone,
// and a response from a different backup are not errors the host can act on: they mean there is no
// more history to page. Anything else is a real failure.
func archiveFetchFailure(err error) any {
	message := err.Error()
	switch {
	case strings.Contains(message, "archive thread was not found"),
		strings.Contains(message, "archive is no longer available"),
		strings.Contains(message, "belongs to a different backup"):
		return MessagePage{Items: []RemoteMessage{}, NextCursor: nil}
	default:
		return errProtocol(message)
	}
}

// decryptArchivePage opens a page. The library fails the entire page on the first message it does
// not understand, so a failed page is retried one message at a time and the unsupported ones are
// skipped with a debug line instead of costing the whole page.
func (m *Messenger) decryptArchivePage(page *securehistory.MessageRange, device *securehistory.Device, threadID string, ownID int64, group bool) []*securehistory.DecryptedMessage {
	if page == nil {
		return nil
	}
	if messages, err := page.Decrypt(device, threadID, ownID, group); err == nil {
		return messages
	}
	out := make([]*securehistory.DecryptedMessage, 0, len(page.Messages))
	for _, archived := range page.Messages {
		single := *page
		single.Messages = []*securehistory.ArchiveMessage{archived}
		messages, err := single.Decrypt(device, threadID, ownID, group)
		if err != nil || len(messages) != 1 {
			m.log.Debug().Err(err).Str("archive_otid", archiveOTID(archived)).
				Msg("Skipping an archive message this client cannot open")
			continue
		}
		out = append(out, messages[0])
	}
	return out
}

func archiveOTID(message *securehistory.ArchiveMessage) string {
	if message == nil {
		return ""
	}
	return message.ID
}
