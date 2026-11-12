package main

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"strings"

	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
)

// sessionVersion is the version of the sidecar's SessionBlob payload. The host treats the
// blob as opaque bytes, so bumping this is only a sidecar concern.
const sessionVersion = 1

// sessionData is what the sidecar puts in a SessionBlob. It carries the messenger.com cookies;
// the E2EE device store lives in $ALLOY_SIDECAR_DATA_DIR/<remote_user_id>/e2ee.db.
type sessionData struct {
	V        int               `json:"v"`
	Platform string            `json:"platform"`
	Cookies  map[string]string `json:"cookies"`
}

func encodeSession(platform string, c *cookies.Cookies) (string, error) {
	values := make(map[string]string, len(c.GetAll()))
	for name, value := range c.GetAll() {
		values[string(name)] = value
	}
	raw, err := json.Marshal(sessionData{V: sessionVersion, Platform: platform, Cookies: values})
	if err != nil {
		return "", fmt.Errorf("encode session: %w", err)
	}
	return base64.StdEncoding.EncodeToString(raw), nil
}

func decodeSession(blob string) (*sessionData, error) {
	raw, err := base64.StdEncoding.DecodeString(strings.TrimSpace(blob))
	if err != nil {
		return nil, fmt.Errorf("session blob is not base64: %w", err)
	}
	var data sessionData
	if err := json.Unmarshal(raw, &data); err != nil {
		return nil, fmt.Errorf("session blob is not JSON: %w", err)
	}
	if data.V != sessionVersion {
		return nil, fmt.Errorf("unsupported session version %d", data.V)
	}
	if data.Platform != "messenger" {
		return nil, fmt.Errorf("session platform %q is not messenger", data.Platform)
	}
	if len(data.Cookies) == 0 {
		return nil, fmt.Errorf("session has no cookies")
	}
	return &data, nil
}

// cookieFields is the login form the host renders. Required matches what messagix needs to load
// the inbox (cookies.FBRequiredCookies); sb and fr are accepted and forwarded when present
// because the messenger.com web session normally has them.
func cookieFields() []CookieField {
	return []CookieField{
		{Name: string(cookies.MetaCookieDatr), Label: "datr cookie", Required: true},
		{Name: string(cookies.FBCookieCUser), Label: "c_user cookie (numeric account id)", Required: true},
		{Name: string(cookies.FBCookieSB), Label: "sb cookie", Required: false},
		{Name: string(cookies.FBCookieXS), Label: "xs cookie", Required: true},
		{Name: string(cookies.FBCookieFR), Label: "fr cookie", Required: false},
	}
}

// cookieLoginHelpURL explains how to export cookies without handing the password to the sidecar.
const cookieLoginHelpURL = "https://docs.mau.fi/bridges/go/meta/authentication.html"

// buildCookies validates the submitted values and wraps them in messagix's cookie type.
//
// The validation rejects missing required cookies, control characters and ';' in values: the
// cookie string is later sent verbatim in HTTP headers, so a stray separator would silently
// truncate or forge other cookies.
func buildCookies(values map[string]string) (*cookies.Cookies, error) {
	cleaned := make(map[cookies.MetaCookieName]string, len(values))
	for name, value := range values {
		name = strings.TrimSpace(name)
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if strings.ContainsAny(value, ";\r\n") {
			return nil, fmt.Errorf("cookie %q contains a character that is not allowed in a header", name)
		}
		cleaned[cookies.MetaCookieName(name)] = value
	}
	var missing []string
	for _, required := range []cookies.MetaCookieName{cookies.FBCookieXS, cookies.FBCookieCUser, cookies.MetaCookieDatr} {
		if cleaned[required] == "" {
			missing = append(missing, string(required))
		}
	}
	if len(missing) > 0 {
		return nil, fmt.Errorf("missing required cookies: %s", strings.Join(missing, ", "))
	}
	c := &cookies.Cookies{Platform: platformMessenger}
	c.UpdateValues(cleaned)
	return c, nil
}

// cookiesFromSession rebuilds the messagix cookie jar from a stored blob.
func cookiesFromSession(data *sessionData) (*cookies.Cookies, error) {
	return buildCookies(data.Cookies)
}
