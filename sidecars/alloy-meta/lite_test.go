package main

import (
	"context"
	"errors"
	"strings"
	"testing"

	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
)

// TestClassifyLiteStep pins the mapping from the Bloks steps mautrix-meta produces to the input the
// connector asks for. Every case is a fake step: nothing here touches the network.
func TestClassifyLiteStep(t *testing.T) {
	tests := []struct {
		name     string
		step     liteStep
		wantKind liteKind
		wantFld  string
		wantVia  string
	}{
		{
			name: "credentials",
			step: liteStep{ID: "fi.mau.meta.messengerlite.email_password", Fields: []liteInputField{
				{ID: "username", Type: "username"},
				{ID: "password", Type: "password"},
			}},
			wantKind: liteCredentials,
		},
		{
			name:     "password re-entry",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.password", Fields: []liteInputField{{ID: "password", Type: "password"}}},
			wantKind: litePassword,
			wantFld:  "password",
		},
		{
			name:     "one-time code by email",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.otp_code", Instructions: "Enter the code we sent to your email", Fields: []liteInputField{{ID: "otp_code", Type: "2fa_code"}}},
			wantKind: liteCode,
			wantFld:  "otp_code",
			wantVia:  "email",
		},
		{
			name:     "sms code",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.sms", Fields: []liteInputField{{ID: "sms_code", Type: "2fa_code"}}},
			wantKind: liteCode,
			wantFld:  "sms_code",
			wantVia:  "sms",
		},
		{
			name:     "authenticator code",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.totp", Fields: []liteInputField{{ID: "totp_code", Type: "2fa_code"}}},
			wantKind: liteCode,
			wantFld:  "totp_code",
			wantVia:  "app",
		},
		{
			name:     "whatsapp code",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.whatsapp", Fields: []liteInputField{{ID: "whatsapp_code", Type: "2fa_code"}}},
			wantKind: liteCode,
			wantFld:  "whatsapp_code",
			wantVia:  "other",
		},
		{
			name:     "display and wait",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.afad_wait"},
			wantKind: liteCode,
			wantVia:  "app",
		},
		{
			name:     "captcha",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.captcha", Fields: []liteInputField{{ID: "captcha_code", Type: "captcha_code"}}},
			wantKind: liteBlocked,
		},
		{
			name:     "two-factor method picker",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.mfa_type", Fields: []liteInputField{{ID: "mfatype", Type: "select", Options: []string{"SMS", "Authenticator"}}}},
			wantKind: liteUnsupported,
		},
		{
			name:     "google oauth",
			step:     liteStep{ID: "fi.mau.meta.messengerlite.google_oauth", Fields: []liteInputField{{ID: "oauth_token"}}},
			wantKind: liteUnsupported,
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			kind, field, via := classifyLiteStep(tc.step)
			if kind != tc.wantKind || field != tc.wantFld || via != tc.wantVia {
				t.Fatalf("classify = (%v, %q, %q), want (%v, %q, %q)", kind, field, via, tc.wantKind, tc.wantFld, tc.wantVia)
			}
		})
	}
}

// scriptedFlow feeds fixed steps to the loop, recording what the connector submitted.
func scriptedFlow(polls ...litePoll) (*liteFlow, *[]map[string]string) {
	var sent []map[string]string
	next := 0
	return &liteFlow{poll: func(_ context.Context, input map[string]string) (litePoll, error) {
		sent = append(sent, input)
		if next >= len(polls) {
			return litePoll{}, errors.New("script exhausted")
		}
		poll := polls[next]
		next++
		return poll, nil
	}}, &sent
}

// TestLiteFlowCodeLoop covers the multi-step loop: one code step, then the same step coming back
// with the network's rejection, which ends the attempt instead of prompting again.
func TestLiteFlowCodeLoop(t *testing.T) {
	codeStep := liteStep{
		ID:           "fi.mau.meta.messengerlite.otp_code",
		Instructions: "Enter the code we sent to your email",
		Fields:       []liteInputField{{ID: "otp_code", Type: "2fa_code"}},
	}
	rejected := liteStep{
		ID:           codeStep.ID,
		Instructions: "Facebook rejected that code. " + codeStep.Instructions,
		Fields:       codeStep.Fields,
	}
	flow, sent := scriptedFlow(litePoll{Step: &codeStep}, litePoll{Step: &rejected})

	out := flow.run(context.Background(), map[string]string{"username": "ada@example.com", "password": "hunter2"})
	if out.Failure != nil || out.Step.Type != "code" || out.Step.Via != "email" {
		t.Fatalf("first turn = %+v, want a code step over email", out)
	}

	out = flow.run(context.Background(), map[string]string{flow.codeField(): "111111"})
	if out.Failure == nil || out.Failure.Type != "invalid_input" || out.Failure.Field == nil || *out.Failure.Field != "code" {
		t.Fatalf("rejected code turn = %+v, want invalid_input on code", out)
	}
	if !strings.Contains(out.Failure.Message, "rejected that code") {
		t.Fatalf("rejection lost the network's reason: %q", out.Failure.Message)
	}
	if len(*sent) != 2 || (*sent)[1]["otp_code"] != "111111" {
		t.Fatalf("code was not submitted to the otp_code field: %v", *sent)
	}
}

// TestLiteFlowRejectedCredentials reports a second credentials prompt as a rejected password.
func TestLiteFlowRejectedCredentials(t *testing.T) {
	credentials := liteStep{
		ID:           "fi.mau.meta.messengerlite.email_password",
		Instructions: "Enter your Facebook credentials.",
		Fields: []liteInputField{
			{ID: "username", Type: "username"},
			{ID: "password", Type: "password"},
		},
	}
	rejected := credentials
	rejected.Instructions = "Invalid username or password. " + credentials.Instructions
	flow, _ := scriptedFlow(litePoll{Step: &credentials}, litePoll{Step: &rejected})

	if out := flow.run(context.Background(), map[string]string{}); out.Step.Type != "credentials" {
		t.Fatalf("first turn = %+v, want credentials", out)
	}
	out := flow.run(context.Background(), map[string]string{"username": "ada", "password": "wrong"})
	if out.Failure == nil || out.Failure.Type != "invalid_input" || out.Failure.Field == nil || *out.Failure.Field != "password" {
		t.Fatalf("rejected credentials turn = %+v, want invalid_input on password", out)
	}
	if !strings.Contains(out.Failure.Message, "Invalid username or password") {
		t.Fatalf("rejection lost the network's reason: %q", out.Failure.Message)
	}
}

// TestLiteFlowDone ends the loop with the cookies the flow produced.
func TestLiteFlowDone(t *testing.T) {
	finished := &cookies.Cookies{Platform: types.MessengerLiteAndroid}
	flow, sent := scriptedFlow(litePoll{Cookies: finished})
	out := flow.run(context.Background(), map[string]string{"username": "ada", "password": "hunter2"})
	if out.Failure != nil || out.Cookies != finished {
		t.Fatalf("finished turn = %+v, want the flow's cookies", out)
	}
	if len(*sent) != 1 || (*sent)[0]["username"] != "ada" {
		t.Fatalf("credentials were not submitted: %v", *sent)
	}
}

// TestLiteFlowUnsupportedStep fails rather than rendering a step the contract cannot express.
func TestLiteFlowUnsupportedStep(t *testing.T) {
	picker := liteStep{
		ID:     "fi.mau.meta.messengerlite.mfa_type",
		Fields: []liteInputField{{ID: "mfatype", Type: "select", Options: []string{"SMS"}}},
	}
	flow, _ := scriptedFlow(litePoll{Step: &picker})
	out := flow.run(context.Background(), map[string]string{})
	if out.Failure == nil || out.Failure.Type != "not_supported" {
		t.Fatalf("select step = %+v, want not_supported", out)
	}
}

func TestLiteFailureRateLimited(t *testing.T) {
	failure := liteFailure(errors.New("requesting login: " + httpclient.ErrRateLimited.Error()))
	if failure.Type != "rate_limited" || failure.RetryAfterMs == nil || *failure.RetryAfterMs != 30_000 {
		t.Fatalf("rate limit classified as %+v", failure)
	}
	if wrapped := liteFailure(errors.Join(errors.New("rpc"), httpclient.ErrRateLimited)); wrapped.Type != "rate_limited" {
		t.Fatalf("wrapped rate limit classified as %+v", wrapped)
	}
}

func TestRedactSecret(t *testing.T) {
	failure := failureInvalidInput("code", "the code 111111 was rejected")
	got := redactSecret(failure, "111111")
	if strings.Contains(got.Message, "111111") || !strings.Contains(got.Message, "<redacted>") {
		t.Fatalf("secret not redacted: %q", got.Message)
	}
	if short := redactSecret(failure, "11"); short.Message != failure.Message {
		t.Fatalf("short secrets must not be replaced: %q", short.Message)
	}
}
