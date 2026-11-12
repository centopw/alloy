package main

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"io"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/rs/zerolog"

	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
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
	want := `{"type":"hello","protocol_version":1,"network":"messenger","capabilities":{"edit":true,"edit_window_secs":900,"delete_for_everyone":true,"reactions":true,"max_reactions_per_user":1,"replies":true,"threads":false,"typing":true,"read_receipts":true,"delivered_receipts":true,"voice":true,"polls":false,"location":false,"max_upload_bytes":26214400,"formatted_text":false}}`
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
	want := `{"type":"event","event":{"type":"message","message":{"remote_chat_id":"1234567","remote_msg_id":"mid.$abc","txn_id":null,"sender_remote_id":"42","is_own":false,"sent_at":1700000000000,"edited_at":null,"kind":"text","body":"hello","formatted":null,"reply_to_remote_msg_id":null,"media":null}}}`
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
