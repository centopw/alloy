// Command alloy-meta is Alloy's Facebook Messenger connector sidecar.
//
// It speaks the alloy-connector NDJSON protocol on stdin/stdout and writes diagnostics to
// stderr only: stdout is reserved for protocol frames.
package main

import (
	"bufio"
	"encoding/json"
	"io"
	"os"
	"strings"
	"sync"

	"github.com/rs/zerolog"

	_ "modernc.org/sqlite"
)

// writer serialises frames: requests are handled concurrently, but stdout has a single writer.
type writer struct {
	mu  sync.Mutex
	enc *json.Encoder
}

func newWriter(out io.Writer) *writer {
	enc := json.NewEncoder(out)
	enc.SetEscapeHTML(false)
	return &writer{enc: enc}
}

func (w *writer) write(frame any) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.enc.Encode(frame)
}

func main() {
	logger := zerolog.New(os.Stderr).With().Timestamp().Logger()
	cfg, err := loadSettings(os.Getenv)
	if err != nil {
		// stdout is reserved for protocol frames, so the host reads this on stderr and surfaces it
		// as the network's error instead of a sidecar that starts with the wrong settings.
		logger.Error().Err(err).Msg("Invalid startup configuration")
		os.Exit(2)
	}
	logger.Info().
		Str("platform", cfg.platformName()).
		Str("proxy", cfg.describeProxy()).
		Msg("Messenger sidecar starting")
	out := newWriter(os.Stdout)
	messenger := newMessenger(logger, out, cfg)

	if err := out.write(NewHello()); err != nil {
		logger.Err(err).Msg("Failed to write the hello frame")
		return
	}

	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var request Request
		if err := json.Unmarshal([]byte(line), &request); err != nil {
			logger.Warn().Err(err).Msg("Discarding malformed request line")
			continue
		}
		if request.Type != "request" {
			logger.Warn().Str("frame_type", request.Type).Msg("Discarding non-request frame from the host")
			continue
		}
		go func(request Request) {
			result, werr := messenger.handle(request.Method, request.Params)
			response := Response{Type: "response", ID: request.ID, Result: result, Error: werr}
			if err := out.write(response); err != nil {
				logger.Err(err).Msg("Failed to write a response frame")
			}
		}(request)
	}
	if err := scanner.Err(); err != nil {
		logger.Err(err).Msg("Reading stdin failed")
	}
	_ = messenger.stop()
}
