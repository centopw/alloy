package main

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"strconv"
	"strings"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
)

// beginLogin starts a cookie-import attempt. Messenger has no phone/code login in this sidecar:
// the network accepts a browser session exported from messenger.com.
func (m *Messenger) beginLogin() LoginStep {
	m.mu.Lock()
	m.pendingLogin = true
	m.mu.Unlock()
	return cookiesStep(cookieFields(), cookieLoginHelpURL)
}

func (m *Messenger) submitLogin(ctx context.Context, params json.RawMessage) (any, *WireError) {
	fail := func(reason LoginFailure) (any, *WireError) {
		m.mu.Lock()
		m.pendingLogin = false
		m.mu.Unlock()
		return failedStep(reason), nil
	}
	var p SubmitLoginParams
	if err := json.Unmarshal(params, &p); err != nil {
		return fail(failureInvalidInput("cookies", "malformed submit_login params: "+err.Error()))
	}
	if p.Input.Type != "cookies" {
		return fail(failureNotSupported())
	}

	m.mu.Lock()
	pending := m.pendingLogin
	m.mu.Unlock()
	if !pending {
		return fail(failureInvalidInput("cookies", "no login attempt is in progress; call begin_login first"))
	}

	c, err := buildCookies(p.Input.Values)
	if err != nil {
		return fail(failureInvalidInput("cookies", err.Error()))
	}
	cli := messagix.NewClient(c, m.logger(), &messagix.Config{})
	user, initial, err := cli.LoadMessagesPage(ctx)
	if err != nil {
		return fail(loginFailure(err))
	}
	if cookieUser := c.GetUserID(); cookieUser != 0 && cookieUser != user.GetFBID() {
		return fail(failureInvalidInput("cookies", "the cookies belong to a different account than c_user"))
	}
	session, err := encodeSession("messenger", c)
	if err != nil {
		return fail(failureInvalidInput("cookies", err.Error()))
	}

	remoteID := strconv.FormatInt(user.GetFBID(), 10)
	m.mu.Lock()
	m.cookies = c
	m.cli = cli
	m.initial = initial
	m.ownID = user.GetFBID()
	m.remoteUser = remoteID
	m.display = user.GetName()
	m.loggedOut = false
	m.pendingLogin = false
	m.mu.Unlock()

	return doneStep(remoteID, user.GetName(), session), nil
}

// loginFailure classifies a login error into the four cases the contract distinguishes:
// rejected cookies, a locked account, a rate limit, and everything transport-level.
func loginFailure(err error) LoginFailure {
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
		return failureInvalidInput("cookies", "the cookies are not a logged-in messenger.com session")
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
	return failureInvalidInput("cookies", message)
}
