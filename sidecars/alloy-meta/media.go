package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
)

// fetchMedia writes one attachment to dest_path and returns the bytes written. Plain
// Messenger attachments are HTTP URLs; encrypted ones carry the media keys in remote_ref.
func (m *Messenger) fetchMedia(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p FetchMediaParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("fetch_media params: " + err.Error())
	}
	if strings.TrimSpace(p.DestPath) == "" {
		return nil, errInvalidInput("dest_path must not be empty")
	}
	if p.Media.RemoteRef == "" {
		return nil, errInvalidInput("media has no remote reference")
	}
	if strings.HasPrefix(p.Media.RemoteRef, e2eeRefPrefix) {
		ref, err := decodeE2EEMediaRef(p.Media.RemoteRef)
		if err != nil {
			return nil, errInvalidInput(err.Error())
		}
		return m.fetchE2EEMedia(ctx, ref, p.DestPath)
	}
	if !strings.HasPrefix(p.Media.RemoteRef, "https://") && !strings.HasPrefix(p.Media.RemoteRef, "http://") {
		return nil, errInvalidInput("media reference is not a URL")
	}
	cli, werr := m.requireClient()
	if werr != nil {
		return nil, werr
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, p.Media.RemoteRef, nil)
	if err != nil {
		return nil, errInvalidInput("media reference is not a valid URL: " + err.Error())
	}
	headers := cli.GetHTTP().BuildHeaders(true, false)
	headers.Set("accept", "*/*")
	req.Header = headers
	resp, err := cli.GetHTTP().HTTP.Do(req)
	if err != nil {
		return nil, errNetwork(err.Error())
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, errNetwork(fmt.Sprintf("media download returned HTTP %d", resp.StatusCode))
	}

	file, err := os.Create(p.DestPath)
	if err != nil {
		return nil, errInvalidInput(fmt.Sprintf("cannot create %q: %v", p.DestPath, err))
	}
	written, copyErr := io.Copy(file, resp.Body)
	closeErr := file.Close()
	if copyErr != nil {
		return nil, errNetwork(fmt.Sprintf("media download failed: %v", copyErr))
	}
	if closeErr != nil {
		return nil, errInvalidInput(fmt.Sprintf("cannot finish writing %q: %v", p.DestPath, closeErr))
	}
	return uint64(written), nil
}

// writeMediaFile writes downloaded bytes to dest_path, mapping filesystem failures to
// InvalidInput as the connector contract requires.
func writeMediaFile(destPath string, data []byte) *WireError {
	file, err := os.Create(destPath)
	if err != nil {
		return errInvalidInput(fmt.Sprintf("cannot create %q: %v", destPath, err))
	}
	if _, err := file.Write(data); err != nil {
		_ = file.Close()
		return errInvalidInput(fmt.Sprintf("cannot write %q: %v", destPath, err))
	}
	if err := file.Close(); err != nil {
		return errInvalidInput(fmt.Sprintf("cannot finish writing %q: %v", destPath, err))
	}
	return nil
}
