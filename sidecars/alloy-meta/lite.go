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
	"time"

	"github.com/rs/zerolog"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/bloks"
	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
	"maunium.net/go/mautrix/bridgev2"
)

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
	// liteWait is a step with no input of its own: the network is waiting for the user to finish
	// the sign-in elsewhere (approve the notification on their phone). The host shows the network's
	// instructions and answers with a "continue" input until the flow moves on.
	liteWait
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
	// awaitingCode records that the last returned step asked for a one-time code, which is what
	// makes a challenge answerable with a code on this device.
	awaitingCode bool
	// awaitingApproval records that the last returned step is the network waiting for something
	// only the user can do elsewhere; waitingMessage is what that step told them.
	awaitingApproval bool
	waitingMessage   string
}

// liteLogin owns the messagix client that runs the flow and the flow state itself. The mutex
// keeps two concurrent submit_login calls from interleaving inside the same Bloks state machine.
type liteLogin struct {
	mu   sync.Mutex
	log  zerolog.Logger
	cli  *messagix.Client
	flow *liteFlow
}

// newLiteLogin starts a fresh Messenger Lite attempt on the configured platform variant. The cookie
// jar is empty on purpose: the flow produces its own cookies, and a stale session would leak the
// previous account into the login.
func newLiteLogin(cfg settings, log zerolog.Logger) *liteLogin {
	l := &liteLogin{
		log: log,
		cli: cfg.newMessagixClient(log, &cookies.Cookies{Platform: cfg.platform}),
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

// afadPollWindow bounds one "continue" answer while the network waits for the user to approve the
// sign-in elsewhere. The library blocks inside its approval poll, so the window keeps one
// submit_login from holding the host's request for minutes: the host asks again, and each answer
// is either the next step or the same wait step.
const afadPollWindow = 20 * time.Second

// answerContinue polls the flow once for the approval it is waiting on. Anything the user could
// type is irrelevant to that step, so no field is filled in.
func (l *liteLogin) answerContinue(ctx context.Context) liteOutcome {
	l.mu.Lock()
	defer l.mu.Unlock()
	if !l.flow.awaitingApproval {
		failure := failureInvalidInput("continue", "the login is not waiting for an approval")
		return liteOutcome{Failure: &failure}
	}
	pollCtx, cancel := context.WithTimeout(ctx, afadPollWindow)
	defer cancel()
	return l.flow.run(pollCtx, map[string]string{})
}

// run performs one turn and classifies the result.
func (f *liteFlow) run(ctx context.Context, input map[string]string) liteOutcome {
	polled, err := f.poll(ctx, input)
	if err != nil {
		return f.outcome(err)
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
	// A challenge can only be answered with a code while the flow is waiting for one; every other
	// turn loses that ability.
	f.awaitingCode = kind == liteCode
	if kind != liteWait {
		f.awaitingApproval = false
		f.waitingMessage = ""
	}

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
	case liteWait:
		// The network is waiting for the user, not for an answer here: the same step coming back is
		// the normal polling outcome, so it is never treated as a rejected input.
		f.expect = nil
		f.lastKey = ""
		f.awaitingApproval = true
		f.waitingMessage = waitInstructions(step.Instructions)
		return liteOutcome{Step: waitStep(f.waitingMessage)}
	case liteBlocked:
		// A captcha is a checkpoint: only Meta's own site can render it, so the host points the user
		// there and the attempt stays open for another answer.
		return liteOutcome{Step: challengeStep(challengeCheckpoint, liteCaptchaMessage)}
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
	case liteWait:
		return "wait"
	case liteBlocked:
		return "blocked"
	default:
		return ""
	}
}

// liteCaptchaMessage explains why a captcha is terminal for the sidecar.
const liteCaptchaMessage = "Facebook is asking for a captcha; complete the security check in the official app or on facebook.com, then log in again"

// waitApprovalFallback is shown when Meta sends the approval screen without instructions of its
// own, which the Bloks step almost always carries.
const waitApprovalFallback = "Confirm this sign-in in the Facebook app on your phone; this page continues once you do"

// waitInstructions is the network's own instruction for an approval step, with a local fallback so
// the host never shows an empty waiting screen.
func waitInstructions(text string) string {
	if text = strings.TrimSpace(text); text != "" {
		return text
	}
	return waitApprovalFallback
}

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
	// network waits for the user to approve in the official app; nothing typed here answers it, so
	// it becomes a wait step the host keeps answering with "continue".
	if len(step.Fields) == 0 {
		return liteWait, "", ""
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

// The text the host shows for each challenge. The connector cannot clear any of them, so the copy
// says where the user has to go.
const (
	challengeCheckpointMessage = "Facebook needs to confirm this sign-in on its own site or app: approve it on facebook.com or in the Facebook app, then try again"
	challengeTwoFactorMessage  = "Facebook wants a two-factor code before it will let this device in; enter it below, or approve the sign-in on facebook.com or in the Facebook app, then try again"
	challengeConsentMessage    = "Facebook wants the account holder to accept its consent screens; accept them on facebook.com or in the Facebook app, then try again"
	accountSuspendedMessage    = "Facebook has suspended this account; resolve it on facebook.com or in the Facebook app"
)

// challengeFor reports whether err is one of Meta's own challenges, and which. Only a flow that is
// waiting for a code can turn ErrChallengeRequired into a two-factor challenge: otherwise there is no
// code field to answer with, so it stays the checkpoint form.
//
// The unsupported-MFA dead-ends are checked first: they are the library's own RespError codes, and
// no other answer exists — the user has to add a supported method on Meta's site.
func challengeFor(err error, awaitingCode bool) (kind, message string, ok bool) {
	if message, isUnsupportedMfa := unsupportedMfaChallenge(err); isUnsupportedMfa {
		return challengeUnsupportedMfa, message, true
	}
	var checkpoint bloks.CheckpointError
	switch {
	case errors.As(err, &checkpoint),
		errors.Is(err, httpclient.ErrCheckpointRequired):
		return challengeCheckpoint, challengeCheckpointMessage, true
	case errors.Is(err, httpclient.ErrChallengeRequired):
		if awaitingCode {
			return challengeTwoFactor, challengeTwoFactorMessage, true
		}
		return challengeCheckpoint, challengeCheckpointMessage, true
	case errors.Is(err, httpclient.ErrConsentRequired):
		return challengeConsent, challengeConsentMessage, true
	}
	return "", "", false
}

// outcome classifies an error from a turn of the flow into what the protocol can carry. A challenge
// becomes a step so the attempt stays open — the user can answer a code, switch to cookies, or retry
// once Meta's site has been visited; everything else ends the attempt with the matching failure.
func (f *liteFlow) outcome(err error) liteOutcome {
	// The bounded poll window ended while the network was still waiting for the user: that is not a
	// failure, it is the same wait step, so the host can ask again.
	if f.awaitingApproval && (errors.Is(err, context.DeadlineExceeded) || errors.Is(err, context.Canceled)) {
		return liteOutcome{Step: waitStep(f.waitingMessage)}
	}
	if kind, message, ok := challengeFor(err, f.awaitingCode); ok {
		return liteOutcome{Step: challengeStep(kind, message)}
	}
	failure := liteFailure(err)
	return liteOutcome{Failure: &failure}
}

// liteFailure classifies an error from the Bloks flow into the failures the contract distinguishes:
// a suspension is an account lock, a 429 a rate limit, a rejection of the submitted pair an invalid
// input. Challenges are not failures; [`outcome`] turns them into steps before this runs.
func liteFailure(err error) LoginFailure {
	var respErr bridgev2.RespError
	if errors.As(err, &respErr) && respErr.ErrCode == "FI.MAU.META_AFAD_STOPPED" {
		// The approval this flow was waiting for was denied or expired; Meta's own words say so.
		return failureInvalidInput("continue", respErr.Err)
	}
	switch {
	case errors.Is(err, httpclient.ErrAccountSuspended):
		return failureAccountLocked(accountSuspendedMessage)
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
