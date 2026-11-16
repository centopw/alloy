package main

import (
	"strings"

	"github.com/rs/zerolog"
	zlog "github.com/rs/zerolog/log"
)

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
// so it becomes the filtered logger.
func installGlobalLogger(log zerolog.Logger) {
	zlog.Logger = quietMessagixLog(log)
}
