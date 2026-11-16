package main

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/rs/zerolog"

	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
	"go.mau.fi/mautrix-meta/pkg/messagix/table"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
)

// encodeFrame renders one frame the way the sidecar writes it, without the trailing newline.
func encodeFrame(t *testing.T, frame any) string {
	t.Helper()
	var buf bytes.Buffer
	w := newWriter(&buf)
	if err := w.write(frame); err != nil {
		t.Fatalf("write frame: %v", err)
	}
	return strings.TrimSuffix(buf.String(), "\n")
}

// TestHelloFrame pins the byte-for-byte shape the Rust Frame::Hello deserializer accepts.
func TestHelloFrame(t *testing.T) {
	got := encodeFrame(t, NewHello())
	want := `{"type":"hello","protocol_version":1,"network":"messenger","capabilities":{"edit":true,"edit_window_secs":900,"delete_for_everyone":true,"reactions":true,"max_reactions_per_user":1,"replies":true,"threads":false,"typing":true,"read_receipts":true,"delivered_receipts":true,"voice":true,"polls":false,"location":false,"max_upload_bytes":26214400,"formatted_text":false,"pin_chats":true,"pin_messages":false}}`
	if got != want {
		t.Fatalf("hello frame mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestResponseWithError pins the internally tagged ConnectorError shape and the always-present
// result/error pair.
func TestResponseWithError(t *testing.T) {
	got := encodeFrame(t, Response{
		Type:   "response",
		ID:     7,
		Result: nil,
		Error:  errNotSupported("polls"),
	})
	want := `{"type":"response","id":7,"result":null,"error":{"type":"not_supported","feature":"polls"}}`
	if got != want {
		t.Fatalf("error response mismatch\n got: %s\nwant: %s", got, want)
	}
}

func TestResponseWithRateLimitError(t *testing.T) {
	got := encodeFrame(t, Response{
		Type:   "response",
		ID:     9,
		Result: nil,
		Error:  errRateLimited(1500),
	})
	want := `{"type":"response","id":9,"result":null,"error":{"type":"rate_limited","retry_after_ms":1500}}`
	if got != want {
		t.Fatalf("rate limit response mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestEventMessageFrame pins the event envelope and every Option field of RemoteMessage.
func TestEventMessageFrame(t *testing.T) {
	body := "hello"
	got := encodeFrame(t, EventFrame{
		Type: "event",
		Event: NewMessageEvent(RemoteMessage{
			RemoteChatID:   "1234567",
			RemoteMsgID:    "mid.$abc",
			SenderRemoteID: "42",
			SentAt:         1700000000000,
			Kind:           "text",
			Body:           &body,
		}),
	})
	want := `{"type":"event","event":{"type":"message","message":{"remote_chat_id":"1234567","remote_msg_id":"mid.$abc","txn_id":null,"sender_remote_id":"42","sender":null,"is_own":false,"sent_at":1700000000000,"edited_at":null,"kind":"text","body":"hello","formatted":null,"reply_to_remote_msg_id":null,"media":null,"pinned":false}}}`
	if got != want {
		t.Fatalf("message event mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestEventTypingFrame pins a small event whose field names are easy to get wrong.
func TestEventTypingFrame(t *testing.T) {
	got := encodeFrame(t, EventFrame{
		Type:  "event",
		Event: NewTypingEvent("t_1", "u_1", true),
	})
	want := `{"type":"event","event":{"type":"typing","remote_chat_id":"t_1","remote_user_id":"u_1","typing":true}}`
	if got != want {
		t.Fatalf("typing event mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestEventConnectionOfflineFrame pins the offline state the Rust ConnectionState deserializer
// reads: the 1-based attempt count and the delay the connector itself will wait before the next
// attempt.
func TestEventConnectionOfflineFrame(t *testing.T) {
	got := encodeFrame(t, EventFrame{
		Type:  "event",
		Event: NewConnectionEvent(stateOffline("network unreachable", 3, 8000)),
	})
	want := `{"type":"event","event":{"type":"connection","state":{"type":"offline","reason":"network unreachable","attempt":3,"next_retry_millis":8000}}}`
	if got != want {
		t.Fatalf("offline connection event mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestEventConnectionOfflineWithoutAttempt keeps the report valid for connectors that do not count
// attempts: the host then assumes the first one. A delay with no count still travels, so a banner
// can show it.
func TestEventConnectionOfflineWithoutAttempt(t *testing.T) {
	got := encodeFrame(t, EventFrame{
		Type:  "event",
		Event: NewConnectionEvent(stateOffline("socket closed", 0, 0)),
	})
	want := `{"type":"event","event":{"type":"connection","state":{"type":"offline","reason":"socket closed"}}}`
	if got != want {
		t.Fatalf("uncounted offline connection event mismatch\n got: %s\nwant: %s", got, want)
	}

	got = encodeFrame(t, EventFrame{
		Type:  "event",
		Event: NewConnectionEvent(stateOffline("socket closed", 0, 2000)),
	})
	want = `{"type":"event","event":{"type":"connection","state":{"type":"offline","reason":"socket closed","next_retry_millis":2000}}}`
	if got != want {
		t.Fatalf("delay without an attempt mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestChallengeStepKinds pins the challenge step the host renders as a guide: the step carries the
// kind and the network's own message, and it is a step rather than a failure so the attempt stays
// open for another answer.
func TestChallengeStepKinds(t *testing.T) {
	cases := []struct {
		kind    string
		message string
		want    string
	}{
		{
			kind:    challengeCheckpoint,
			message: challengeCheckpointMessage,
			want:    `{"type":"challenge","kind":"checkpoint","message":"` + challengeCheckpointMessage + `"}`,
		},
		{
			kind:    challengeTwoFactor,
			message: challengeTwoFactorMessage,
			want:    `{"type":"challenge","kind":"two_factor","message":"` + challengeTwoFactorMessage + `"}`,
		},
		{
			kind:    challengeConsent,
			message: challengeConsentMessage,
			want:    `{"type":"challenge","kind":"consent","message":"` + challengeConsentMessage + `"}`,
		},
	}
	for _, c := range cases {
		got := encodeFrame(t, challengeStep(c.kind, c.message))
		if got != c.want {
			t.Fatalf("%s challenge frame mismatch\n got: %s\nwant: %s", c.kind, got, c.want)
		}
	}
}

// TestOnlyChallengesCarryAKind keeps every other step decoding into the shape the contract already
// had: no stray "kind" key.
func TestOnlyChallengesCarryAKind(t *testing.T) {
	for name, step := range map[string]LoginStep{
		"code":       codeStep("sms"),
		"failed":     failedStep(failureAccountLocked(accountSuspendedMessage)),
		"failedIn":   failedStep(failureInvalidInput("code", "incorrect code")),
		"credential": credentialsStep(),
	} {
		got := encodeFrame(t, step)
		if strings.Contains(got, `"kind"`) {
			t.Fatalf("%s step carried a challenge kind: %s", name, got)
		}
	}
}

// TestChallengeKindClassification covers which error becomes which kind, so the wire value and the
// copy cannot drift apart.
func TestChallengeKindClassification(t *testing.T) {
	if kind, message, ok := challengeFor(httpclient.ErrConsentRequired, false); !ok || kind != challengeConsent || message != challengeConsentMessage {
		t.Fatalf("consent error classified as %q/%v", kind, ok)
	}
	if kind, _, ok := challengeFor(httpclient.ErrCheckpointRequired, true); !ok || kind != challengeCheckpoint {
		t.Fatalf("checkpoint error classified as %q/%v", kind, ok)
	}
	// Only a flow waiting for a code can offer the code field the two-factor kind promises.
	if kind, _, ok := challengeFor(httpclient.ErrChallengeRequired, true); !ok || kind != challengeTwoFactor {
		t.Fatalf("challenge while awaiting a code classified as %q/%v", kind, ok)
	}
	if kind, _, ok := challengeFor(httpclient.ErrChallengeRequired, false); !ok || kind != challengeCheckpoint {
		t.Fatalf("challenge without a code step classified as %q/%v", kind, ok)
	}
	if _, _, ok := challengeFor(errors.New("some other failure"), true); ok {
		t.Fatal("an ordinary error must not be a challenge")
	}
	if got := liteFailure(httpclient.ErrAccountSuspended); got.Type != "account_locked" {
		t.Fatalf("suspension classified as %+v", got)
	}
}

// TestChallengeWhileAwaitingCodeIsTwoFactor covers the only case that can be a two-factor
// challenge: the flow has a live code step. Without one the challenge is a checkpoint.
func TestChallengeWhileAwaitingCodeIsTwoFactor(t *testing.T) {
	code := liteStep{
		ID:     "fi.mau.meta.messengerlite.totp",
		Fields: []liteInputField{{ID: "totp_code", Type: "2fa_code"}},
	}
	flow := &liteFlow{poll: func(_ context.Context, _ map[string]string) (litePoll, error) {
		return litePoll{Step: &code}, nil
	}}
	out := flow.run(context.Background(), map[string]string{})
	if out.Failure != nil || out.Step.Type != "code" || out.Step.Via != "app" {
		t.Fatalf("totp step = %+v, want a code step over the app", out)
	}
	if flow.codeField() != "totp_code" {
		t.Fatalf("code answers go to %q, want totp_code", flow.codeField())
	}

	flow.poll = func(_ context.Context, _ map[string]string) (litePoll, error) {
		return litePoll{}, httpclient.ErrChallengeRequired
	}
	out = flow.run(context.Background(), map[string]string{"totp_code": "123456"})
	if out.Failure != nil || out.Step.Type != "challenge" || out.Step.Kind != challengeTwoFactor {
		t.Fatalf("challenge while awaiting a code = %+v, want the two_factor challenge step", out)
	}

	plain := &liteFlow{poll: func(_ context.Context, _ map[string]string) (litePoll, error) {
		return litePoll{}, httpclient.ErrChallengeRequired
	}}
	out = plain.run(context.Background(), map[string]string{})
	if out.Failure != nil || out.Step.Type != "challenge" || out.Step.Kind != challengeCheckpoint {
		t.Fatalf("challenge without a code step = %+v, want the checkpoint challenge step", out)
	}
}

// TestCaptchaStaysAnswerable keeps the captcha a step instead of an account lock: the user can clear
// it on Meta's site and the same attempt continues.
func TestCaptchaStaysAnswerable(t *testing.T) {
	captcha := liteStep{
		ID:     "fi.mau.meta.messengerlite.captcha",
		Fields: []liteInputField{{ID: "captcha_code", Type: "captcha_code"}},
	}
	flow, _ := scriptedFlow(litePoll{Step: &captcha})
	out := flow.run(context.Background(), map[string]string{})
	if out.Failure != nil || out.Step.Type != "challenge" || out.Step.Kind != challengeCheckpoint {
		t.Fatalf("captcha = %+v, want the checkpoint challenge step", out)
	}
}

// TestDoneLoginStep pins the terminal login step the host persists.
func TestDoneLoginStep(t *testing.T) {
	got := encodeFrame(t, doneStep("100012345", "Ada", "AAAA"))
	want := `{"type":"done","remote_user_id":"100012345","display_name":"Ada","session":"AAAA"}`
	if got != want {
		t.Fatalf("done step mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestCredentialsLoginStep pins the first step of the credentials flow and the cookie fallback it
// advertises.
func TestCredentialsLoginStep(t *testing.T) {
	got := encodeFrame(t, credentialsStep())
	want := `{"type":"credentials","username_label":"Email or phone number","alternatives":["cookies"]}`
	if got != want {
		t.Fatalf("credentials step mismatch\n got: %s\nwant: %s", got, want)
	}
}

func TestCodeLoginStep(t *testing.T) {
	got := encodeFrame(t, codeStep("email"))
	// length is Option<u8> in the contract; a missing key decodes as None.
	want := `{"type":"code","via":"email"}`
	if got != want {
		t.Fatalf("code step mismatch\n got: %s\nwant: %s", got, want)
	}
}

func TestPasswordLoginStep(t *testing.T) {
	got := encodeFrame(t, passwordStep())
	want := `{"type":"password"}`
	if got != want {
		t.Fatalf("password step mismatch\n got: %s\nwant: %s", got, want)
	}
}

// TestSwitchToCookies drives the login switch the UI sends when the user picks the cookie fallback.
func TestSwitchToCookies(t *testing.T) {
	m := newMessenger(zerolog.Nop(), newWriter(io.Discard), defaultSettings())
	if step := m.beginLogin(); step.Type != "credentials" {
		t.Fatalf("begin_login returned %q, want credentials", step.Type)
	}
	result, werr := m.submitLogin(context.Background(), json.RawMessage(`{"input":{"type":"switch","to":"cookies"}}`))
	if werr != nil {
		t.Fatalf("switch failed: %v", werr)
	}
	step, ok := result.(LoginStep)
	if !ok || step.Type != "cookies" {
		t.Fatalf("switch returned %#v, want the cookies step", result)
	}
	if step.HelpURL == nil || *step.HelpURL != cookieLoginHelpURL {
		t.Fatalf("cookies step lost its help url: %#v", step)
	}
	m.mu.Lock()
	pending := m.pendingLogin
	m.mu.Unlock()
	if !pending {
		t.Fatal("switching to cookies must keep the attempt open")
	}
}

func TestSwitchToUnknownAlternativeFails(t *testing.T) {
	m := newMessenger(zerolog.Nop(), newWriter(io.Discard), defaultSettings())
	m.beginLogin()
	result, _ := m.submitLogin(context.Background(), json.RawMessage(`{"input":{"type":"switch","to":"qr"}}`))
	step, ok := result.(LoginStep)
	if !ok || step.Type != "failed" || step.Reason == nil || step.Reason.Type != "not_supported" {
		t.Fatalf("unknown alternative returned %#v, want failed/not_supported", result)
	}
}

func TestLoginRequiresBegin(t *testing.T) {
	m := newMessenger(zerolog.Nop(), newWriter(io.Discard), defaultSettings())
	result, _ := m.submitLogin(context.Background(), json.RawMessage(`{"input":{"type":"code","code":"1"}}`))
	step, ok := result.(LoginStep)
	if !ok || step.Type != "failed" || step.Reason == nil || step.Reason.Type != "invalid_input" {
		t.Fatalf("submit without begin_login returned %#v, want failed/invalid_input", result)
	}
}

func TestCookiesLoginStep(t *testing.T) {
	got := encodeFrame(t, cookiesStep(cookieFields(), cookieLoginHelpURL))
	if !strings.HasPrefix(got, `{"type":"cookies","fields":[`) {
		t.Fatalf("cookies step has the wrong prefix: %s", got)
	}
	if !strings.Contains(got, `"help_url":"`+cookieLoginHelpURL+`"`) {
		t.Fatalf("cookies step is missing the help URL: %s", got)
	}
	for _, name := range []string{"datr", "c_user", "sb", "xs", "fr"} {
		if !strings.Contains(got, `"name":"`+name+`"`) {
			t.Fatalf("cookies step is missing %q: %s", name, got)
		}
	}
}

func TestBuildCookiesRequiresSessionCookies(t *testing.T) {
	_, err := buildCookies(map[string]string{"c_user": "1", "datr": "d"})
	if err == nil || !strings.Contains(err.Error(), "xs") {
		t.Fatalf("expected a missing-xs error, got %v", err)
	}
}

func TestBuildCookiesRejectsHeaderInjection(t *testing.T) {
	_, err := buildCookies(map[string]string{
		"xs":     "a;other=1",
		"c_user": "1",
		"datr":   "d",
	})
	if err == nil || !strings.Contains(err.Error(), "not allowed") {
		t.Fatalf("expected a header-injection error, got %v", err)
	}
}

func TestBuildCookiesAcceptsOptionalCookies(t *testing.T) {
	c, err := buildCookies(map[string]string{
		"xs":     "token",
		"c_user": "100012345",
		"datr":   "datr-value",
		"sb":     "sb-value",
		"fr":     "fr-value",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got := c.GetUserID(); got != 100012345 {
		t.Fatalf("user id = %d, want 100012345", got)
	}
	if !c.IsLoggedIn() {
		t.Fatal("cookies should count as logged in")
	}
}

func TestSessionRoundTrip(t *testing.T) {
	c, err := buildCookies(map[string]string{
		"xs":     "token",
		"c_user": "100012345",
		"datr":   "datr-value",
	})
	if err != nil {
		t.Fatalf("build cookies: %v", err)
	}
	blob, err := encodeSession(c, nil)
	if err != nil {
		t.Fatalf("encode session: %v", err)
	}
	decoded, err := decodeSession(blob)
	if err != nil {
		t.Fatalf("decode session: %v", err)
	}
	if decoded.Cookies["xs"] != "token" || decoded.Platform != "messenger" {
		t.Fatalf("session round trip lost fields: %+v", decoded)
	}
	if decoded.Native != nil {
		t.Fatalf("cookie login must not store a native session: %+v", decoded.Native)
	}
	if _, err := decodeSession("not base64!"); err == nil {
		t.Fatal("expected an error for a malformed blob")
	}
}

// TestSessionRoundTripNative covers the version 2 payload the Messenger Lite flow writes.
func TestSessionRoundTripNative(t *testing.T) {
	c := &cookies.Cookies{Platform: types.MessengerLiteAndroid}
	c.UpdateValues(map[cookies.MetaCookieName]string{
		cookies.FBCookieXS:     "token",
		cookies.FBCookieCUser:  "100012345",
		cookies.MetaCookieDatr: "datr-value",
	})
	native := &types.NativeSession{AccessToken: "access", AppID: "app", DeviceID: uuid.MustParse("11111111-1111-1111-1111-111111111111")}
	blob, err := encodeSession(c, native)
	if err != nil {
		t.Fatalf("encode session: %v", err)
	}
	decoded, err := decodeSession(blob)
	if err != nil {
		t.Fatalf("decode session: %v", err)
	}
	if decoded.V != 2 || decoded.Platform != "messenger-lite-android" {
		t.Fatalf("native session envelope lost fields: %+v", decoded)
	}
	restored, restoredNative, err := cookiesFromSession(decoded)
	if err != nil {
		t.Fatalf("cookies from session: %v", err)
	}
	if restored.Platform != types.MessengerLiteAndroid {
		t.Fatalf("restored platform = %v, want messenger lite android", restored.Platform)
	}
	if restoredNative == nil || restoredNative.AccessToken != "access" || restoredNative.DeviceID.String() != "11111111-1111-1111-1111-111111111111" {
		t.Fatalf("restored native session = %+v", restoredNative)
	}
}

// TestSessionVersion1StillRestores keeps blobs written before the native session existed usable.
func TestSessionVersion1StillRestores(t *testing.T) {
	blob := base64.StdEncoding.EncodeToString([]byte(
		`{"v":1,"platform":"messenger","cookies":{"xs":"token","c_user":"100012345","datr":"d"}}`,
	))
	data, err := decodeSession(blob)
	if err != nil {
		t.Fatalf("decode version 1 session: %v", err)
	}
	if data.Platform != "messenger" || data.Native != nil {
		t.Fatalf("version 1 session decoded wrong: %+v", data)
	}
}

func TestSessionRejectsForeignPlatform(t *testing.T) {
	blob := base64.StdEncoding.EncodeToString([]byte(
		`{"v":2,"platform":"instagram","cookies":{"xs":"token","c_user":"1","datr":"d"}}`,
	))
	if _, err := decodeSession(blob); err == nil {
		t.Fatal("expected an error for a foreign platform")
	}
}

// TestOneToOneChatUsesContactRow pins that a one-to-one chat, whose thread row has no name or
// picture, takes both from the other person's contact row, and that a group does not.
func TestOneToOneChatUsesContactRow(t *testing.T) {
	m := newMessenger(zerolog.Nop(), newWriter(io.Discard), defaultSettings())
	tbl := &table.LSTable{
		LSDeleteThenInsertThread: []*table.LSDeleteThenInsertThread{
			{ThreadKey: 42, ThreadType: table.ENCRYPTED_OVER_WA_ONE_TO_ONE},
			{ThreadKey: 43, ThreadType: table.GROUP_THREAD},
		},
		LSVerifyContactRowExists: []*table.LSVerifyContactRowExists{
			{ContactId: 42, Name: "Ada", ProfilePictureUrl: "https://cdn.example/ada.jpg"},
			{ContactId: 43, Name: "Not the group", ProfilePictureUrl: "https://cdn.example/no.jpg"},
		},
	}
	m.mu.Lock()
	m.applyThreadTableLocked(tbl)
	direct := m.chatForThreadLocked(m.threads[42])
	group := m.chatForThreadLocked(m.threads[43])
	m.mu.Unlock()

	if direct.Title != "Ada" || direct.Avatar == nil || direct.Avatar.RemoteRef != "https://cdn.example/ada.jpg" {
		t.Fatalf("direct chat ignored its contact row: %+v avatar=%+v", direct, direct.Avatar)
	}
	if group.Title != "43" || group.Avatar != nil {
		t.Fatalf("group chat took a contact row: %+v", group)
	}
}

// TestMessageCarriesSenderProfile pins that a group message names its sender from the contact
// row, live and in a history page, and that a sender without a contact row stays a bare id.
func TestMessageCarriesSenderProfile(t *testing.T) {
	m := newMessenger(zerolog.Nop(), newWriter(io.Discard), defaultSettings())
	tbl := &table.LSTable{
		LSDeleteThenInsertThread: []*table.LSDeleteThenInsertThread{
			{ThreadKey: 43, ThreadType: table.GROUP_THREAD},
		},
		LSVerifyContactRowExists: []*table.LSVerifyContactRowExists{
			{ContactId: 7, Name: "Ada", ProfilePictureUrl: "https://cdn.example/ada.jpg"},
		},
		LSInsertMessage: []*table.LSInsertMessage{
			{ThreadKey: 43, MessageId: "mid.1", SenderId: 7, Text: "hi", TimestampMs: 1},
			{ThreadKey: 43, MessageId: "mid.2", SenderId: 8, Text: "yo", TimestampMs: 2},
		},
	}
	m.mu.Lock()
	m.applyThreadTableLocked(tbl)
	events := m.messageEventsLocked(tbl)
	m.mu.Unlock()

	if len(events) != 2 {
		t.Fatalf("expected two message events, got %d", len(events))
	}
	named := events[0].(MessageEvent).Message
	if named.Sender == nil || named.Sender.RemoteUserID != named.SenderRemoteID ||
		named.Sender.DisplayName != "Ada" || named.Sender.Avatar == nil ||
		named.Sender.Avatar.RemoteRef != "https://cdn.example/ada.jpg" {
		t.Fatalf("message lost its sender profile: %+v", named.Sender)
	}
	if bare := events[1].(MessageEvent).Message; bare.Sender != nil {
		t.Fatalf("a sender without a contact row got a profile: %+v", bare.Sender)
	}
}

// TestMembersFollowParticipantRows pins the member list a thread's participant rows produce:
// admins carry the role, a removed participant is gone, and the wire shape matches RemoteMember.
func TestMembersFollowParticipantRows(t *testing.T) {
	m := newMessenger(zerolog.Nop(), newWriter(io.Discard), defaultSettings())
	m.mu.Lock()
	defer m.mu.Unlock()
	m.applyThreadTableLocked(&table.LSTable{
		LSVerifyContactRowExists: []*table.LSVerifyContactRowExists{
			{ContactId: 7, Name: "Ada"},
		},
		LSAddParticipantIdToGroupThread: []*table.LSAddParticipantIdToGroupThread{
			{ThreadKey: 43, ContactId: 9},
			{ThreadKey: 43, ContactId: 7, IsAdmin: true},
			{ThreadKey: 43, ContactId: 8},
			{ThreadKey: 44, ContactId: 5},
		},
	})
	m.applyThreadTableLocked(&table.LSTable{
		LSRemoveParticipantFromThread: []*table.LSRemoveParticipantFromThread{
			{ThreadKey: 43, ParticipantId: 8},
		},
		LSUpdateThreadParticipantAdminStatus: []*table.LSUpdateThreadParticipantAdminStatus{
			{ThreadKey: 43, ContactId: 9, IsAdmin: true},
			{ThreadKey: 43, ContactId: 7, IsAdmin: false},
		},
	})

	raw, err := json.Marshal(m.membersLocked(m.members[43]))
	if err != nil {
		t.Fatal(err)
	}
	want := `[{"user":{"remote_user_id":"7","display_name":"Ada","avatar":null},"role":null},` +
		`{"user":{"remote_user_id":"9","display_name":"","avatar":null},"role":"admin"}]`
	if string(raw) != want {
		t.Fatalf("member list mismatch\n got: %s\nwant: %s", raw, want)
	}

	m.applyThreadTableLocked(&table.LSTable{
		LSRemoveAllParticipantsForThread: []*table.LSRemoveAllParticipantsForThread{{ThreadKey: 43}},
	})
	if len(m.members[43]) != 0 || len(m.members[44]) != 1 {
		t.Fatalf("clearing one thread touched the wrong members: %+v", m.members)
	}
}

// TestListMembersNeedsASession pins that the method is dispatched and refuses a logged-out call.
func TestListMembersNeedsASession(t *testing.T) {
	m := newMessenger(zerolog.Nop(), newWriter(io.Discard), defaultSettings())
	_, werr := m.handle("list_members", json.RawMessage(`{"remote_chat_id":"43"}`))
	if werr == nil || werr.Type != "unauthorized" {
		t.Fatalf("expected unauthorized, got %+v", werr)
	}
}

// pinnedMessenger is a logged-in messenger with two plain threads and no network: enough for
// the pin methods, which never touch messagix. Pins are written under dataDir.
func pinnedMessenger(t *testing.T, dataDir string, out io.Writer) *Messenger {
	t.Helper()
	m := newMessenger(zerolog.Nop(), newWriter(out), defaultSettings())
	m.dataDir = dataDir
	m.cli = defaultSettings().newMessagixClient(zerolog.Nop(), &cookies.Cookies{Platform: types.MessengerLiteAndroid})
	m.mu.Lock()
	m.applyThreadTableLocked(&table.LSTable{
		LSDeleteThenInsertThread: []*table.LSDeleteThenInsertThread{
			{ThreadKey: 42, ThreadType: table.ONE_TO_ONE},
			{ThreadKey: 43, ThreadType: table.GROUP_THREAD},
		},
	})
	m.loadPinsLocked()
	m.mu.Unlock()
	return m
}

// pinOrders renders what list_chats would say about pins: chat id to (is_pinned, pin_order).
func pinOrders(m *Messenger) map[string][2]int {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := make(map[string][2]int)
	for _, t := range m.allThreadsLocked() {
		chat := m.chatForThreadLocked(t)
		order := -1
		if chat.PinOrder != nil {
			order = int(*chat.PinOrder)
		}
		pinned := 0
		if chat.IsPinned {
			pinned = 1
		}
		out[chat.RemoteChatID] = [2]int{pinned, order}
	}
	return out
}

// TestPinnedChatsOrder pins the sidecar-local pin list: a new pin goes to the top, a reorder
// replaces the list, an unpin closes the gap, unknown chats are refused and every change is
// announced with the frozen event shapes.
func TestPinnedChatsOrder(t *testing.T) {
	var events bytes.Buffer
	m := pinnedMessenger(t, t.TempDir(), &events)

	if _, werr := m.handle("set_chat_pinned", json.RawMessage(`{"remote_chat_id":"43","pinned":true}`)); werr != nil {
		t.Fatalf("pin 43: %+v", werr)
	}
	if _, werr := m.handle("set_chat_pinned", json.RawMessage(`{"remote_chat_id":"42","pinned":true}`)); werr != nil {
		t.Fatalf("pin 42: %+v", werr)
	}
	got := pinOrders(m)
	if got["42"] != [2]int{1, 0} || got["43"] != [2]int{1, 1} {
		t.Fatalf("a new pin did not go to the top: %v", got)
	}

	if _, werr := m.handle("reorder_pinned_chats", json.RawMessage(`{"remote_chat_ids":["43","42"]}`)); werr != nil {
		t.Fatalf("reorder: %+v", werr)
	}
	got = pinOrders(m)
	if got["43"] != [2]int{1, 0} || got["42"] != [2]int{1, 1} {
		t.Fatalf("reorder did not replace the list: %v", got)
	}

	// Idempotent: pinning a pinned chat changes nothing and emits nothing.
	before := events.Len()
	if _, werr := m.handle("set_chat_pinned", json.RawMessage(`{"remote_chat_id":"43","pinned":true}`)); werr != nil {
		t.Fatalf("re-pin 43: %+v", werr)
	}
	if events.Len() != before {
		t.Fatalf("re-pinning emitted an event: %s", events.String()[before:])
	}

	if _, werr := m.handle("set_chat_pinned", json.RawMessage(`{"remote_chat_id":"43","pinned":false}`)); werr != nil {
		t.Fatalf("unpin 43: %+v", werr)
	}
	got = pinOrders(m)
	if got["43"] != [2]int{0, -1} || got["42"] != [2]int{1, 0} {
		t.Fatalf("unpin left a gap: %v", got)
	}

	for _, params := range []string{
		`{"remote_chat_id":"99","pinned":true}`,
		`{"remote_chat_id":"not-a-chat","pinned":true}`,
	} {
		if _, werr := m.handle("set_chat_pinned", json.RawMessage(params)); werr == nil || werr.Type != "invalid_input" {
			t.Fatalf("expected invalid_input for %s, got %+v", params, werr)
		}
	}
	if _, werr := m.handle("reorder_pinned_chats", json.RawMessage(`{"remote_chat_ids":["42","99"]}`)); werr == nil || werr.Type != "invalid_input" {
		t.Fatalf("expected invalid_input for an unknown chat, got %+v", werr)
	}
	if got = pinOrders(m); got["42"] != [2]int{1, 0} {
		t.Fatalf("a refused reorder changed the list: %v", got)
	}

	want := `{"type":"event","event":{"type":"chat_pinned","remote_chat_id":"43","pinned":true}}` + "\n" +
		`{"type":"event","event":{"type":"chat_pinned","remote_chat_id":"42","pinned":true}}` + "\n" +
		`{"type":"event","event":{"type":"pinned_chats_reordered","remote_chat_ids":["43","42"]}}` + "\n" +
		`{"type":"event","event":{"type":"chat_pinned","remote_chat_id":"43","pinned":false}}` + "\n"
	if events.String() != want {
		t.Fatalf("pin events mismatch\n got: %swant: %s", events.String(), want)
	}
}

// TestPinsSurviveReload pins that the list is written to pins.json and read back by the next
// messenger over the same data directory, and that logout removes it.
func TestPinsSurviveReload(t *testing.T) {
	dataDir := t.TempDir()
	m := pinnedMessenger(t, dataDir, io.Discard)
	if _, werr := m.handle("reorder_pinned_chats", json.RawMessage(`{"remote_chat_ids":["43","42"]}`)); werr != nil {
		t.Fatalf("reorder: %+v", werr)
	}
	raw, err := os.ReadFile(filepath.Join(dataDir, pinsFile))
	if err != nil {
		t.Fatalf("pins.json was not written: %v", err)
	}
	if string(raw) != `["43","42"]` {
		t.Fatalf("pins.json mismatch: %s", raw)
	}

	reloaded := pinnedMessenger(t, dataDir, io.Discard)
	got := pinOrders(reloaded)
	if got["43"] != [2]int{1, 0} || got["42"] != [2]int{1, 1} {
		t.Fatalf("pins did not survive the reload: %v", got)
	}

	// The test client never connected; drop it so logout does not disconnect a bare socket.
	reloaded.mu.Lock()
	reloaded.cli = nil
	reloaded.mu.Unlock()
	reloaded.logout(context.Background())
	if _, err := os.Stat(filepath.Join(dataDir, pinsFile)); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("logout kept pins.json: %v", err)
	}
	if _, werr := reloaded.handle("set_chat_pinned", json.RawMessage(`{"remote_chat_id":"43","pinned":true}`)); werr == nil || werr.Type != "unauthorized" {
		t.Fatalf("expected unauthorized after logout, got %+v", werr)
	}
}

// TestSetMessagePinnedNotSupported pins that message pins are refused with the capability name
// the host checks, so it falls back to a local pin.
func TestSetMessagePinnedNotSupported(t *testing.T) {
	m := pinnedMessenger(t, t.TempDir(), io.Discard)
	_, werr := m.handle("set_message_pinned", json.RawMessage(`{"remote_chat_id":"43","remote_msg_id":"mid.1","pinned":true}`))
	if werr == nil || werr.Type != "not_supported" || werr.Feature != "pin_messages" {
		t.Fatalf("expected not_supported(pin_messages), got %+v", werr)
	}
}
