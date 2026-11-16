package main

import (
	"bytes"
	"errors"
	"strings"
	"testing"

	"github.com/rs/zerolog"
	"go.mau.fi/mautrix-meta/pkg/messagix/dgw"
)

func TestQuietMessagixLogDropsOnlySyncNoise(t *testing.T) {
	var buf bytes.Buffer
	log := quietMessagixLog(zerolog.New(&buf).Level(zerolog.DebugLevel))

	log.Warn().Msg("Skipping dependency with unrecognized type")
	log.Warn().Msg("Failed to set int64")
	log.Info().Msg("Syncing initial data via graphql")
	log.Error().Msg("Failed to set the socket up")
	log.Warn().Str("component", "x").Msg("Something the host should see")

	out := buf.String()
	if strings.Contains(out, "Skipping dependency") {
		t.Errorf("sync dependency noise was logged:\n%s", out)
	}
	if strings.Contains(out, "Failed to set int64") {
		t.Errorf("table column noise was logged:\n%s", out)
	}
	if !strings.Contains(out, "Syncing initial data via graphql") {
		t.Errorf("info line was dropped:\n%s", out)
	}
	if !strings.Contains(out, "Failed to set the socket up") {
		t.Errorf("error with a filtered prefix was dropped:\n%s", out)
	}
	if !strings.Contains(out, "Something the host should see") {
		t.Errorf("unrelated warning was dropped:\n%s", out)
	}
}

func TestClosedSocketMapsToTransport(t *testing.T) {
	werr := mapMetaError(dgw.ErrSocketNotOpen)
	if werr == nil {
		t.Fatal("no error mapped")
	}
	if werr.Type != "network" {
		t.Errorf("type = %q, want network", werr.Type)
	}
	wrapped := mapMetaError(errors.Join(dgw.ErrSocketNotOpen, errors.New("send")))
	if wrapped == nil || wrapped.Type != "network" {
		t.Errorf("wrapped socket error type = %v, want network", wrapped)
	}
}
