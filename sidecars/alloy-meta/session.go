package main

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"strings"

	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
)

// sessionVersion is the version of the sidecar's SessionBlob payload. The host treats the
// blob as opaque bytes, so bumping this is only a sidecar concern. Version 2 adds the native
// session the Messenger Lite flow produces; version 1 blobs are still restored.
const sessionVersion = 2

// sessionVersionMin is the oldest payload the sidecar still reads.
const sessionVersionMin = 1

// sessionData is what the sidecar puts in a SessionBlob. It carries the cookies for the platform
// the account logged in with and, for the Lite flow, the native session; the E2EE device store
// lives in $ALLOY_SIDECAR_DATA_DIR/<remote_user_id>/e2ee.db.
type sessionData struct {
	V        int                  `json:"v"`
	Platform string               `json:"platform"`
	Cookies  map[string]string    `json:"cookies"`
	Native   *types.NativeSession `json:"native_session,omitempty"`
}

// encodeSession serialises the cookies and, when the account logged in through Messenger Lite, the
// native session. The platform is part of the payload because it decides the messagix endpoints and
// whether a native session applies at all.
func encodeSession(c *cookies.Cookies, native *types.NativeSession) (string, error) {
	if c == nil {
		return "", fmt.Errorf("encode session: no cookies")
	}
	values := make(map[string]string, len(c.GetAll()))
	for name, value := range c.GetAll() {
		values[string(name)] = value
	}
	if native != nil && (native.AccessToken == "" || native.AppID == "") {
		native = nil
	}
	raw, err := json.Marshal(sessionData{
		V:        sessionVersion,
		Platform: c.Platform.String(),
		Cookies:  values,
		Native:   native,
	})
	if err != nil {
		return "", fmt.Errorf("encode session: %w", err)
	}
	return base64.StdEncoding.EncodeToString(raw), nil
}

// sessionPlatform is the platform a stored blob was created for. Both messenger.com cookie import
// and the Messenger Lite flow are accepted; anything else is a foreign or corrupt blob.
func sessionPlatform(name string) (types.Platform, error) {
	switch name {
	case types.Messenger.String():
		return types.Messenger, nil
	case types.MessengerLiteAndroid.String():
		return types.MessengerLiteAndroid, nil
	default:
		return types.Unset, fmt.Errorf("session platform %q is not messenger", name)
	}
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
	if data.V < sessionVersionMin || data.V > sessionVersion {
		return nil, fmt.Errorf("unsupported session version %d", data.V)
	}
	if _, err := sessionPlatform(data.Platform); err != nil {
		return nil, err
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
	cleaned, err := cleanCookieValues(values)
	if err != nil {
		return nil, err
	}
	return buildCookiesFor(platformMessenger, cleaned), nil
}

// buildCookiesFor wraps cleaned values in messagix's cookie type for one platform.
func buildCookiesFor(platform types.Platform, values map[cookies.MetaCookieName]string) *cookies.Cookies {
	c := &cookies.Cookies{Platform: platform}
	c.UpdateValues(values)
	return c
}

// cleanCookieValues validates submitted cookies and returns them keyed by messagix's cookie names.
func cleanCookieValues(values map[string]string) (map[cookies.MetaCookieName]string, error) {
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
	return cleaned, nil
}

// cookiesFromSession rebuilds the messagix cookie jar and the native session from a stored blob.
func cookiesFromSession(data *sessionData) (*cookies.Cookies, *types.NativeSession, error) {
	platform, err := sessionPlatform(data.Platform)
	if err != nil {
		return nil, nil, err
	}
	cleaned, err := cleanCookieValues(data.Cookies)
	if err != nil {
		return nil, nil, err
	}
	return buildCookiesFor(platform, cleaned), data.Native, nil
}
