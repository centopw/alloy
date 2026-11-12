package main

import (
	"fmt"
	"net/url"
	"strings"

	"github.com/rs/zerolog"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
)

// Environment variables the host sets when it spawns the sidecar. Nothing about the network is
// compiled in: the platform the login presents and the proxy the connection uses are runtime
// configuration, written by Alloy's Settings → Networks → Messenger.
const (
	envPlatform = "ALLOY_META_PLATFORM"
	envProxy    = "ALLOY_META_PROXY"
)

// The accepted ALLOY_META_PLATFORM values, and what a sidecar started without one uses.
const (
	platformAndroid = "android"
	platformIOS     = "ios"
)

// settings is the runtime configuration the host passes in the environment.
type settings struct {
	// platform is the Messenger Lite variant the login logs in as, and the platform every client
	// built for that login carries.
	platform types.Platform
	// proxy is nil when no proxy is configured, which means a direct connection.
	proxy *url.URL
	// config carries that proxy into the transport of every messagix client.
	config *messagix.Config
}

// defaultSettings is the configuration with no environment at all: Android Messenger Lite over a
// direct connection, which is what the sidecar had compiled in before the variables existed.
func defaultSettings() settings {
	return settings{platform: types.MessengerLiteAndroid, config: &messagix.Config{}}
}

// loadSettings reads and validates the configuration, so a typo fails at startup with a message
// the host can show instead of silently falling back to a default the user did not choose.
func loadSettings(getenv func(string) string) (settings, error) {
	platform, err := parsePlatform(getenv(envPlatform))
	if err != nil {
		return settings{}, err
	}
	rawProxy := getenv(envProxy)
	proxy, err := parseProxy(rawProxy)
	if err != nil {
		return settings{}, err
	}
	cfg := &messagix.Config{}
	// Compile the proxy now rather than at the first request: a URL the transport cannot dial is a
	// startup error, not a connection failure the user has to interpret later.
	clientSettings, err := cfg.ClientSettings.WithProxy(proxyAddress(proxy))
	if err != nil {
		return settings{}, fmt.Errorf("%s=%q is not usable as a proxy: %w", envProxy, redactProxy(rawProxy), err)
	}
	cfg.ClientSettings = clientSettings
	return settings{platform: platform, proxy: proxy, config: cfg}, nil
}

// parsePlatform maps ALLOY_META_PLATFORM to the Messenger Lite variant the login uses. The empty
// value is Android, the variant the sidecar used before the setting existed: Android's login
// response also carries a native session (access token + device ids) that the session-for-app
// endpoint can exchange later, so it is the one worth persisting.
func parsePlatform(raw string) (types.Platform, error) {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "", platformAndroid:
		return types.MessengerLiteAndroid, nil
	case platformIOS:
		return types.MessengerLiteIOS, nil
	default:
		return types.Unset, fmt.Errorf("%s=%q is not a platform: use %q (default) or %q", envPlatform, raw, platformAndroid, platformIOS)
	}
}

// parseProxy validates ALLOY_META_PROXY. The empty value means a direct connection.
func parseProxy(raw string) (*url.URL, error) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return nil, nil
	}
	u, err := url.Parse(trimmed)
	if err != nil {
		// url.Parse's own error quotes the input, credentials included, so it is not forwarded.
		return nil, fmt.Errorf("%s=%q is not a URL", envProxy, redactProxy(trimmed))
	}
	switch scheme := strings.ToLower(u.Scheme); scheme {
	case "http", "https", "socks5", "socks5h":
		u.Scheme = scheme
	default:
		return nil, fmt.Errorf("%s=%q uses scheme %q: use http://, https:// or socks5://", envProxy, redactProxy(trimmed), u.Scheme)
	}
	if u.Hostname() == "" {
		return nil, fmt.Errorf("%s=%q has no host: give one, like socks5://127.0.0.1:1080", envProxy, redactProxy(trimmed))
	}
	return u, nil
}

// proxyAddress is the string the transport wants: empty means no proxy.
func proxyAddress(u *url.URL) string {
	if u == nil {
		return ""
	}
	return u.String()
}

// newMessagixClient builds every messagix client the sidecar uses — the Messenger Lite login, the
// inbox load after login, restore, the socket and media — so the proxy cannot be forgotten on one
// path. The proxy goes into the client settings because messagix captures its websocket dialer when
// it builds the client: applying it only afterwards would leave the socket direct.
func (s settings) newMessagixClient(log zerolog.Logger, c *cookies.Cookies) *messagix.Client {
	cli := messagix.NewClient(c, log, s.config)
	if s.proxy != nil {
		addr := s.proxy.String()
		// Mirror mautrix-meta (pkg/connector/login.go getMessagixClient): the HTTP client asks for
		// the proxy again when a request fails, and re-applies it on the same client.
		cli.GetHTTP().GetNewProxy = func(string) (string, error) { return addr, nil }
		if !cli.GetHTTP().UpdateProxy("startup") {
			log.Error().Str("proxy", redactProxy(addr)).Msg("Failed to apply the proxy")
		}
	}
	return cli
}

// describeProxy is what the startup log says about the proxy: "direct" when none is configured,
// otherwise the URL with any credentials replaced, so the line never carries a password.
func (s settings) describeProxy() string {
	if s.proxy == nil {
		return "direct"
	}
	return redactProxy(s.proxy.String())
}

// platformName is the platform as the environment names it.
func (s settings) platformName() string {
	switch s.platform {
	case types.MessengerLiteAndroid:
		return platformAndroid
	case types.MessengerLiteIOS:
		return platformIOS
	default:
		return s.platform.String()
	}
}

// redactProxy replaces the userinfo of a proxy URL, or of text that was meant to be one, with
// "***". It works on the authority section of the raw string, so a malformed URL is redacted too.
func redactProxy(raw string) string {
	schemeEnd := strings.Index(raw, "://")
	prefix, authority := "", raw
	if schemeEnd >= 0 {
		prefix, authority = raw[:schemeEnd+3], raw[schemeEnd+3:]
	}
	rest := ""
	if end := strings.IndexAny(authority, "/?#"); end >= 0 {
		authority, rest = authority[:end], authority[end:]
	}
	at := strings.LastIndex(authority, "@")
	if at < 0 {
		return raw
	}
	return prefix + "***@" + authority[at+1:] + rest
}
