package main

import (
	"strings"

	"github.com/rs/zerolog"
	zlog "github.com/rs/zerolog/log"
)

// dependencyOriginField is the JSON key the sidecar stamps on every line written by a bundled
// dependency through its logger. The host forwards those lines at debug at most, however the
// dependency labelled them: libsignal and whatsmeow log every failed MAC and retry probe at
// warn/error, and Alloy reports the real event once itself. Lines without the marker are the
// sidecar's own reports and keep their level.
const dependencyOriginField = "origin"

// dependencyOriginValue is the only value of dependencyOriginField the host treats as a marker.
const dependencyOriginValue = "dependency"

// dependencyOriginHook stamps that marker. It runs only for loggers handed to third-party code;
// m.log, which writes the sidecar's own reports, is never wrapped with it.
type dependencyOriginHook struct{}

func (dependencyOriginHook) Run(e *zerolog.Event, level zerolog.Level, message string) {
	if e == nil {
		return
	}
	e.Str(dependencyOriginField, dependencyOriginValue)
}

// dependencyLogger marks a logger as belonging to a bundled dependency.
func dependencyLogger(log zerolog.Logger) zerolog.Logger {
	return log.Hook(dependencyOriginHook{})
}

// messagixNoisePrefixes are the warnings mautrix-meta emits for every row of every sync payload:
// a dependency or table column its incomplete Messenger model does not implement. They repeat
// thousands of times a minute during the initial sync and say nothing the host can act on, so they
// are dropped. Errors from the same library still reach the log.
var messagixNoisePrefixes = []string{
	"Skipping dependency",
	"Unknown dependency in sp",
	"Failed to set ",
	"Syncing initial data",
}

// messagixNoiseHook drops those warnings from the client's own logger. It runs only for the logger
// messagix holds; the sidecar's own warnings are untouched.
type messagixNoiseHook struct{}

func (messagixNoiseHook) Run(e *zerolog.Event, level zerolog.Level, message string) {
	if level != zerolog.WarnLevel || e == nil {
		return
	}
	for _, prefix := range messagixNoisePrefixes {
		if strings.HasPrefix(message, prefix) {
			e.Discard()
			return
		}
	}
}

// quietMessagixLog is the logger handed to messagix: the sidecar's own, minus its sync-table noise.
func quietMessagixLog(log zerolog.Logger) zerolog.Logger {
	return log.Hook(messagixNoiseHook{})
}

// installGlobalLogger claims zerolog's process-wide logger, which mautrix-meta's table decoder uses
// directly instead of the client's logger. Left alone it writes its own JSON to stderr at trace
// level: every sync row, with whole payloads. The sidecar process has no other user of that global,
// so it becomes the filtered, dependency-marked logger.
func installGlobalLogger(log zerolog.Logger) {
	zlog.Logger = dependencyLogger(quietMessagixLog(log))
}
