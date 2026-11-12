package main

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"strconv"
	"strings"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
)

// loginFail ends the attempt with a Failed step.
type loginFail func(LoginFailure) (any, *WireError)

// beginLogin starts a Messenger Lite login: the account's email/phone and password, answered by
// the sidecar's own Bloks flow. Cookie import stays available as the documented fallback, which is
// what the credentials step advertises in its alternatives.
func (m *Messenger) beginLogin() LoginStep {
	m.mu.Lock()
	m.pendingLogin = true
	m.lite = nil
	m.mu.Unlock()
	return credentialsStep()
}

// submitLogin answers the last step. Credentials, passwords and codes all continue the same flow;
// a switch to cookies abandons it.
func (m *Messenger) submitLogin(ctx context.Context, params json.RawMessage) (any, *WireError) {
	fail := func(reason LoginFailure) (any, *WireError) {
		m.endLogin()
		return failedStep(reason), nil
	}

	var p SubmitLoginParams
	if err := json.Unmarshal(params, &p); err != nil {
		return fail(failureInvalidInput("credentials", "malformed submit_login params: "+err.Error()))
	}

	m.mu.Lock()
	pending := m.pendingLogin
	m.mu.Unlock()
	if !pending {
		return fail(failureInvalidInput("credentials", "no login attempt is in progress; call begin_login first"))
	}

	switch p.Input.Type {
	case "credentials":
		return m.submitCredentials(ctx, p.Input, fail)
	case "password":
		return m.submitLitePassword(ctx, p.Input.Password, fail)
	case "code":
		return m.submitLiteCode(ctx, p.Input.Code, fail)
	case "switch":
		if p.Input.To != "cookies" {
			return fail(failureNotSupported())
		}
		m.mu.Lock()
		m.lite = nil
		m.mu.Unlock()
		return cookiesStep(cookieFields(), cookieLoginHelpURL), nil
	case "cookies":
		return m.submitCookies(ctx, p.Input.Values, fail)
	default:
		return fail(failureNotSupported())
	}
}

// submitCredentials runs the Messenger Lite login with the account's email/username and password.
func (m *Messenger) submitCredentials(ctx context.Context, input LoginInput, fail loginFail) (any, *WireError) {
	username := strings.TrimSpace(input.Username)
	if username == "" {
		return fail(failureInvalidInput("username", "enter the account's email address, phone number or username"))
	}
	if input.Password == "" {
		return fail(failureInvalidInput("password", "enter the account's password"))
	}
	out := m.liteForAttempt().answerCredentials(ctx, username, input.Password)
	return m.settleLite(ctx, out, input.Password, fail)
}

// submitLitePassword answers the flow's stand-alone password prompt.
func (m *Messenger) submitLitePassword(ctx context.Context, password string, fail loginFail) (any, *WireError) {
	if password == "" {
		return fail(failureInvalidInput("password", "enter the account's password"))
	}
	out := m.liteForAttempt().answerPassword(ctx, password)
	return m.settleLite(ctx, out, password, fail)
}

// submitLiteCode answers the flow's one-time code prompt. Two-factor, SMS, WhatsApp, backup and
// email codes all arrive here; the flow decides which field the answer belongs to.
func (m *Messenger) submitLiteCode(ctx context.Context, code string, fail loginFail) (any, *WireError) {
	code = strings.TrimSpace(code)
	if code == "" {
		return fail(failureInvalidInput("code", "enter the code the network sent"))
	}
	out := m.liteForAttempt().answerCode(ctx, code)
	return m.settleLite(ctx, out, code, fail)
}

// liteForAttempt returns the flow for this login attempt, starting one on the first answer.
func (m *Messenger) liteForAttempt() *liteLogin {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.lite == nil {
		m.lite = newLiteLogin(m.logger())
	}
	return m.lite
}

// settleLite maps one turn of the flow onto the protocol: a step keeps the attempt open, cookies
// finish it, a failure ends it.
func (m *Messenger) settleLite(ctx context.Context, out liteOutcome, secret string, fail loginFail) (any, *WireError) {
	if out.Failure != nil {
		return fail(redactSecret(*out.Failure, secret))
	}
	if out.Cookies != nil {
		return m.finishLiteLogin(ctx, out.Cookies, fail)
	}
	return out.Step, nil
}

// finishLiteLogin turns the cookies the Lite flow produced into a logged-in connector. The inbox
// load is the same check the cookie import does, so a session that cannot read messages is rejected
// before the host persists it.
func (m *Messenger) finishLiteLogin(ctx context.Context, c *cookies.Cookies, fail loginFail) (any, *WireError) {
	m.mu.Lock()
	lite := m.lite
	m.mu.Unlock()
	var native *types.NativeSession
	if lite != nil {
		native = lite.nativeSession()
	}

	cli := messagix.NewClient(c, m.logger(), &messagix.Config{})
	cli.MessengerLite.SetNativeSession(native)
	user, initial, err := cli.LoadMessagesPage(ctx)
	if err != nil {
		return fail(loginFailure(err, "password"))
	}
	if cookieUser := c.GetUserID(); cookieUser != 0 && cookieUser != user.GetFBID() {
		return fail(failureInvalidInput("password", "the session belongs to a different account than the login"))
	}
	session, err := encodeSession(c, native)
	if err != nil {
		return fail(failureInvalidInput("password", err.Error()))
	}

	remoteID := strconv.FormatInt(user.GetFBID(), 10)
	m.mu.Lock()
	m.cookies = c
	m.cli = cli
	m.initial = initial
	m.native = native
	m.ownID = user.GetFBID()
	m.remoteUser = remoteID
	m.display = user.GetName()
	m.loggedOut = false
	m.pendingLogin = false
	m.lite = nil
	m.mu.Unlock()

	return doneStep(remoteID, user.GetName(), session), nil
}

// submitCookies is the fallback: it validates the exported messenger.com cookies by loading the
// inbox and keeps the same session blob format.
func (m *Messenger) submitCookies(ctx context.Context, values map[string]string, fail loginFail) (any, *WireError) {
	c, err := buildCookies(values)
	if err != nil {
		return fail(failureInvalidInput("cookies", err.Error()))
	}
	cli := messagix.NewClient(c, m.logger(), &messagix.Config{})
	user, initial, err := cli.LoadMessagesPage(ctx)
	if err != nil {
		return fail(loginFailure(err, "cookies"))
	}
	if cookieUser := c.GetUserID(); cookieUser != 0 && cookieUser != user.GetFBID() {
		return fail(failureInvalidInput("cookies", "the cookies belong to a different account than c_user"))
	}
	session, err := encodeSession(c, nil)
	if err != nil {
		return fail(failureInvalidInput("cookies", err.Error()))
	}

	remoteID := strconv.FormatInt(user.GetFBID(), 10)
	m.mu.Lock()
	m.cookies = c
	m.cli = cli
	m.initial = initial
	m.native = nil
	m.ownID = user.GetFBID()
	m.remoteUser = remoteID
	m.display = user.GetName()
	m.loggedOut = false
	m.pendingLogin = false
	m.lite = nil
	m.mu.Unlock()

	return doneStep(remoteID, user.GetName(), session), nil
}

// endLogin closes the attempt: the next begin_login starts a fresh Bloks flow.
func (m *Messenger) endLogin() {
	m.mu.Lock()
	m.pendingLogin = false
	m.lite = nil
	m.mu.Unlock()
}

// loginFailure classifies a login error into the four cases the contract distinguishes:
// rejected input, a locked account, a rate limit, and everything transport-level. field names the
// input the user should correct.
func loginFailure(err error, field string) LoginFailure {
	switch {
	case errors.Is(err, httpclient.ErrCheckpointRequired),
		errors.Is(err, httpclient.ErrChallengeRequired),
		errors.Is(err, httpclient.ErrConsentRequired),
		errors.Is(err, httpclient.ErrAccountSuspended):
		return failureAccountLocked("Facebook requires you to confirm this login on its own site or app")
	case errors.Is(err, httpclient.ErrRateLimited):
		return failureRateLimited(30_000)
	case errors.Is(err, httpclient.ErrTokenInvalidated),
		errors.Is(err, httpclient.ErrUserIDIsZero):
		return failureInvalidInput(field, "the session is not a logged-in messenger.com session")
	}
	var netErr net.Error
	if errors.As(err, &netErr) || errors.Is(err, context.DeadlineExceeded) {
		return failureNetwork(err.Error())
	}
	message := err.Error()
	if strings.Contains(message, "no such host") || strings.Contains(message, "connection refused") ||
		strings.Contains(message, "connection reset") || strings.Contains(message, "tls:") {
		return failureNetwork(message)
	}
	return failureInvalidInput(field, message)
}
