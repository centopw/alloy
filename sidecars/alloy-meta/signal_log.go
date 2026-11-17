package main

import (
	"github.com/rs/zerolog"
	siglog "go.mau.fi/libsignal/logger"
)

// libsignalLogger routes the Signal library's diagnostics to stderr through the sidecar's own
// logger. The library's default logger prints to stdout, which is the NDJSON protocol channel, so
// every warning it emitted (a MAC mismatch, for example) corrupted a frame and the host dropped it.
type libsignalLogger struct{ log zerolog.Logger }

func (l libsignalLogger) Debug(caller, message string) {
	l.log.Debug().Str("caller", caller).Msg(message)
}

func (l libsignalLogger) Info(caller, message string) {
	l.log.Info().Str("caller", caller).Msg(message)
}

func (l libsignalLogger) Warning(caller, message string) {
	l.log.Warn().Str("caller", caller).Msg(message)
}

func (l libsignalLogger) Error(caller, message string) {
	l.log.Error().Str("caller", caller).Msg(message)
}

// Configure is part of the library's Loggable interface; the sidecar's log level already decides
// what is written, so there is nothing left for it to configure.
func (l libsignalLogger) Configure(string) {}

// installSignalLogger makes libsignal log through the sidecar, never on stdout.
func installSignalLogger(log zerolog.Logger) {
	l := libsignalLogger{log: dependencyLogger(log.With().Str("component", "libsignal").Logger())}
	// Setup takes a pointer to the interface value, so the concrete logger goes behind one first.
	var target siglog.Loggable = l
	siglog.Setup(&target)
}
