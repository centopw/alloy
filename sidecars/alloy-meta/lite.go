// Messenger Lite username/password login.
//
// mautrix-meta logs in to Messenger without a browser: a pure-Go Bloks interpreter replays the
// official Android app's CAA login flow, encrypts the password with the key served by
// FetchLightspeedKey, and hands back session cookies. The sidecar reuses that engine
// (messagix.MessengerLite.DoLoginSteps) and translates its bridgev2-flavoured steps into the
// connector's own LoginStep vocabulary; none of the bridge's login machinery is imported.
package main

import (
	"context"
	"errors"
	"net"
	"strings"
	"sync"

	"github.com/rs/zerolog"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/bloks"
	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
)

// litePlatform is the platform variant mautrix-meta uses for its Android Messenger Lite login. The
// bridge offers iOS and Android; Android is the variant whose login response also carries a native
// session (access token + device ids) that the session-for-app endpoint can exchange later, so it is
// the one worth persisting.
const litePlatform = types.MessengerLiteAndroid

// liteUsernameLabel is the label for the username field. Messenger accounts are Facebook accounts,
// so an email address, a phone number or the username all work.
const liteUsernameLabel = "Email or phone number"

// liteInputField is the sidecar's copy of one Bloks input field.
type liteInputField struct {
	ID      string
	Name    string
	Type    string
	Options []string
}

// liteStep is the sidecar's copy of one Bloks login step: what the state machine wants next,
// without bridgev2's types.
type liteStep struct {
	ID           string
	Instructions string
	Fields       []liteInputField
}

// litePoll is one DoLoginSteps result: a step that needs more input, or the finished cookies.
type litePoll struct {
	Step    *liteStep
	Cookies *cookies.Cookies
}

// liteOutcome is what one turn of the flow produced: the next step, the finished cookies, or a
// terminal failure.
type liteOutcome struct {
	Step    LoginStep
	Cookies *cookies.Cookies
	Failure *LoginFailure
}

// liteKind is the classification of a Bloks step by the input it asks for.
type liteKind int

const (
	// liteUnsupported is a step the connector has no way to render (a select, a captcha token, an
	// OAuth redirect). The flow ends with not_supported.
	liteUnsupported liteKind = iota
	// liteCredentials asks for the account email/username and password.
	liteCredentials
	// litePassword asks for the password again on its own.
	litePassword
	// liteCode asks for a one-time code.
	liteCode
	// liteBlocked is a captcha: only a human on Meta's own site can clear it.
	liteBlocked
)

// liteFlow drives the Bloks state machine across submit_login calls. poll is the only part that
// touches the network, which keeps the multi-step logic testable.
type liteFlow struct {
	poll func(ctx context.Context, userInput map[string]string) (litePoll, error)
	// expect holds the field IDs the last returned step asked for; the next submit_login answers
	// them.
	expect []string
	// lastKey identifies the last returned step, so the same step coming back is reported as a
	// rejection instead of being rendered again.
	lastKey string
}

// liteLogin owns the messagix client that runs the flow and the flow state itself. The mutex
// keeps two concurrent submit_login calls from interleaving inside the same Bloks state machine.
type liteLogin struct {
	mu   sync.Mutex
	log  zerolog.Logger
	cli  *messagix.Client
	flow *liteFlow
}

// newLiteLogin starts a fresh Messenger Lite attempt. The cookie jar is empty on purpose: the flow
// produces its own cookies, and a stale session would leak the previous account into the login.
func newLiteLogin(log zerolog.Logger) *liteLogin {
	l := &liteLogin{
		log: log,
		cli: messagix.NewClient(&cookies.Cookies{Platform: litePlatform}, log, &messagix.Config{}),
	}
	l.flow = &liteFlow{poll: l.poll}
	return l
}

// poll runs one turn of the Bloks state machine and copies the returned step into our own shape.
// Accessing the fields of the returned bridgev2.LoginStep does not need the bridgev2 package: the
// value is only ever selected from, never named.
func (l *liteLogin) poll(ctx context.Context, userInput map[string]string) (litePoll, error) {
	step, newCookies, err := l.cli.MessengerLite.DoLoginSteps(l.log.WithContext(ctx), userInput)
	if err != nil {
		return litePoll{}, err
	}
	if newCookies != nil {
		return litePoll{Cookies: newCookies}, nil
	}
	if step == nil {
		return litePoll{}, nil
	}
	out := &liteStep{ID: step.StepID, Instructions: step.Instructions}
	if step.UserInputParams != nil {
		for _, field := range step.UserInputParams.Fields {
			out.Fields = append(out.Fields, liteInputField{
				ID:      field.ID,
				Name:    field.Name,
				Type:    string(field.Type),
				Options: field.Options,
			})
		}
	}
	return litePoll{Step: out}, nil
}

// nativeSession is the access token the flow obtained, when the platform produced one.
func (l *liteLogin) nativeSession() *types.NativeSession {
	if l == nil {
		return nil
	}
	return l.cli.MessengerLite.GetNativeSession()
}

// answerCredentials submits the account email/username and password.
func (l *liteLogin) answerCredentials(ctx context.Context, username, password string) liteOutcome {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.flow.run(ctx, map[string]string{"username": username, "password": password})
}

// answerPassword submits a password the flow asked for on its own.
func (l *liteLogin) answerPassword(ctx context.Context, password string) liteOutcome {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.flow.run(ctx, map[string]string{"password": password})
}

// answerCode submits a one-time code into the field the last step asked for.
func (l *liteLogin) answerCode(ctx context.Context, code string) liteOutcome {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.flow.run(ctx, map[string]string{l.flow.codeField(): code})
}

// run performs one turn and classifies the result.
func (f *liteFlow) run(ctx context.Context, input map[string]string) liteOutcome {
	polled, err := f.poll(ctx, input)
	if err != nil {
		failure := liteFailure(err)
		return liteOutcome{Failure: &failure}
	}
	if polled.Cookies != nil {
		return liteOutcome{Cookies: polled.Cookies}
	}
	if polled.Step == nil {
		failure := failureNetwork("the Messenger Lite login finished without returning a session")
		return liteOutcome{Failure: &failure}
	}
	return f.mapStep(*polled.Step)
}

// mapStep turns one Bloks step into the connector step the host renders. A step that comes back
// after it was answered means the network rejected the answer: the state machine re-prompts with
// its own reason in Instructions, so that is reported as an invalid input instead of a second
// identical prompt.
func (f *liteFlow) mapStep(step liteStep) liteOutcome {
	kind, field, via := classifyLiteStep(step)
	key := step.ID + "|" + kindName(kind)
	repeat := key != "" && key == f.lastKey

	switch kind {
	case liteCredentials:
		if repeat {
			failure := failureInvalidInput("password", step.Instructions)
			return liteOutcome{Failure: &failure}
		}
		f.expect = []string{"username", "password"}
		f.lastKey = key
		return liteOutcome{Step: credentialsStep()}
	case litePassword:
		if repeat {
			failure := failureInvalidInput("password", step.Instructions)
			return liteOutcome{Failure: &failure}
		}
		f.expect = []string{"password"}
		f.lastKey = key
		return liteOutcome{Step: passwordStep()}
	case liteCode:
		if repeat {
			failure := failureInvalidInput("code", step.Instructions)
			return liteOutcome{Failure: &failure}
		}
		f.expect = []string{field}
		f.lastKey = key
		return liteOutcome{Step: codeStep(via)}
	case liteBlocked:
		failure := failureAccountLocked(liteCaptchaMessage)
		return liteOutcome{Failure: &failure}
	default:
		failure := failureNotSupported()
		return liteOutcome{Failure: &failure}
	}
}

// codeField is the Bloks input the next code submission must be written to.
func (f *liteFlow) codeField() string {
	if len(f.expect) == 1 && f.expect[0] != "" {
		return f.expect[0]
	}
	return "otp_code"
}

func kindName(kind liteKind) string {
	switch kind {
	case liteCredentials:
		return "credentials"
	case litePassword:
		return "password"
	case liteCode:
		return "code"
	case liteBlocked:
		return "blocked"
	default:
		return ""
	}
}

// liteCaptchaMessage explains why a captcha is terminal for the sidecar.
const liteCaptchaMessage = "Facebook is asking for a captcha; complete the security check in the official app or on facebook.com, then log in again"

// classifyLiteStep decides what the connector asks for from the field IDs and types the Bloks state
// machine put in the step. The field ID decides first and the component type second, so a renamed
// label does not change the flow.
func classifyLiteStep(step liteStep) (kind liteKind, field, via string) {
	for _, f := range step.Fields {
		switch f.ID {
		case "username":
			return liteCredentials, "", ""
		case "password":
			return litePassword, "password", ""
		case "otp_code", "sms_code", "totp_code", "backup_code", "whatsapp_code":
			return liteCode, f.ID, codeChannel(step, f.ID)
		case "captcha_code", "recaptcha_token":
			return liteBlocked, "", ""
		case "oauth_token", "mfatype", "contact_point", "account", "dialog_action", "confirmation_action":
			return liteUnsupported, "", ""
		}
	}
	for _, f := range step.Fields {
		switch f.Type {
		case "username":
			return liteCredentials, "", ""
		case "password":
			return litePassword, "password", ""
		case "2fa_code":
			return liteCode, f.ID, codeChannel(step, f.ID)
		case "captcha_code":
			return liteBlocked, "", ""
		case "select":
			return liteUnsupported, "", ""
		}
	}
	// A display-and-wait step (approve the login on another device) carries no fields at all. The
	// network waits for the user to approve in the official app, so the step is offered as an app
	// code: whatever the host sends is unused and the flow advances once the approval lands.
	if len(step.Fields) == 0 {
		return liteCode, "", "app"
	}
	return liteUnsupported, "", ""
}

// codeChannel is the CodeChannel wire value for a code step: the field decides, the page text breaks
// the tie for the generic one-time-code screen.
func codeChannel(step liteStep, field string) string {
	switch field {
	case "sms_code":
		return "sms"
	case "whatsapp_code":
		return "other"
	case "totp_code":
		return "app"
	}
	text := strings.ToLower(step.Instructions)
	switch {
	case strings.Contains(text, "email"):
		return "email"
	case strings.Contains(text, "whatsapp"):
		return "other"
	case strings.Contains(text, "sms") || strings.Contains(text, "text message"):
		return "sms"
	}
	return "app"
}

// liteFailure classifies an error from the Bloks flow into the failures the contract distinguishes.
// A checkpoint, challenge, consent prompt or suspension means only Meta's own site can unblock the
// account; a 429 is a rate limit; a rejection of the submitted pair is an invalid input.
func liteFailure(err error) LoginFailure {
	var checkpoint bloks.CheckpointError
	switch {
	case errors.As(err, &checkpoint),
		errors.Is(err, httpclient.ErrCheckpointRequired),
		errors.Is(err, httpclient.ErrChallengeRequired),
		errors.Is(err, httpclient.ErrConsentRequired),
		errors.Is(err, httpclient.ErrAccountSuspended):
		return failureAccountLocked("Facebook requires you to confirm this login on its own site or app")
	case errors.Is(err, httpclient.ErrRateLimited):
		return failureRateLimited(30_000)
	case errors.Is(err, context.DeadlineExceeded):
		return failureNetwork("the Messenger Lite login did not finish in time")
	}
	message := err.Error()
	switch {
	case strings.Contains(message, "Invalid username or password"),
		strings.Contains(message, "incorrect password"):
		return failureInvalidInput("password", message)
	case strings.Contains(message, "isn’t connected to an account"),
		strings.Contains(message, "isn't connected to an account"):
		return failureInvalidInput("username", message)
	case strings.Contains(message, "Phone number login is not supported"),
		strings.Contains(message, "valid username"):
		// The username field invites a phone number, but Meta's CAA flow only accepts an email
		// address, a username or a Facebook user id.
		return failureInvalidInput("username", message)
	case strings.Contains(message, "unexpected HTTP status 429"),
		strings.Contains(message, "rate-limit"):
		return failureRateLimited(30_000)
	}
	var netErr net.Error
	if errors.As(err, &netErr) ||
		strings.Contains(message, "connection refused") ||
		strings.Contains(message, "no such host") ||
		strings.Contains(message, "tls:") {
		return failureNetwork(message)
	}
	return failureInvalidInput("credentials", message)
}

// redactSecret removes a submitted secret from a failure message. The network's own error text is
// shown to the user and logged, and some Bloks failures quote the value that was submitted.
func redactSecret(failure LoginFailure, secret string) LoginFailure {
	if len(secret) < 3 || !strings.Contains(failure.Message, secret) {
		return failure
	}
	failure.Message = strings.ReplaceAll(failure.Message, secret, "<redacted>")
	return failure
}
