package main

import (
	"bytes"
	"encoding/json"
	"io"
	"testing"

	"github.com/rs/zerolog"
	waLog "go.mau.fi/whatsmeow/util/log"
)

// decodeLogLine returns the fields of the single zerolog object the logger wrote.
func decodeLogLine(t *testing.T, buf *bytes.Buffer) map[string]any {
	t.Helper()
	var line map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(buf.Bytes()), &line); err != nil {
		t.Fatalf("log line is not one JSON object: %v\n%s", err, buf.String())
	}
	return line
}

func TestDependencyLoggersStampTheirOrigin(t *testing.T) {
	cases := []struct {
		name  string
		write func(zerolog.Logger)
	}{
		{
			name: "libsignal",
			write: func(base zerolog.Logger) {
				libsignalLogger{
					log: dependencyLogger(base.With().Str("component", "libsignal").Logger()),
				}.Warning("SessionCipher.go:263", "mismatching MAC in signal message")
			},
		},
		{
			name: "whatsmeow",
			write: func(base zerolog.Logger) {
				waLog.Zerolog(dependencyLogger(base.With().Str("component", "whatsmeow").Logger())).
					Warnf("failed to decrypt prekey message: %v", "no valid sessions")
			},
		},
		{
			name: "messagix",
			write: func(base zerolog.Logger) {
				messenger := newMessenger(base, newWriter(io.Discard), settings{})
				log := messenger.logger()
				log.Warn().Msg("Something the host should see")
			},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			var buf bytes.Buffer
			tc.write(zerolog.New(&buf).Level(zerolog.DebugLevel))
			line := decodeLogLine(t, &buf)
			if got := line[dependencyOriginField]; got != dependencyOriginValue {
				t.Errorf("%s line origin = %v, want %q: %s",
					tc.name, got, dependencyOriginValue, buf.String())
			}
		})
	}
}

func TestOwnLogLinesCarryNoDependencyOrigin(t *testing.T) {
	var buf bytes.Buffer
	log := zerolog.New(&buf).Level(zerolog.DebugLevel).With().Timestamp().Logger()

	// The shape handleE2EEUndecryptable emits: the one Alloy-level report of a failed message.
	log.Warn().
		Str("message_id", "3EB0C1D2").
		Str("chat", "e2ee:100009129333612@msgr").
		Str("sender", "100009129333612:111@msgr").
		Bool("unavailable", false).
		Str("decrypt_fail_mode", "").
		Msg("Failed to decrypt a WhatsApp message; a retry receipt was sent to the sender")

	line := decodeLogLine(t, &buf)
	if origin, ok := line[dependencyOriginField]; ok {
		t.Errorf("the sidecar's own report carries %s=%v", dependencyOriginField, origin)
	}
	if got := line["level"]; got != "warn" {
		t.Errorf("the sidecar's own report level = %v, want warn", got)
	}
}
