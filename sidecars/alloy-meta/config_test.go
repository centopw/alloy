package main

import (
	"strings"
	"testing"

	"github.com/rs/zerolog"

	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
)

// envReader answers getenv from a map, standing in for the host's environment.
func envReader(values map[string]string) func(string) string {
	return func(key string) string { return values[key] }
}

// TestPlatformFromEnv covers the default, both accepted variants and their case.
func TestPlatformFromEnv(t *testing.T) {
	for _, tc := range []struct {
		raw  string
		want types.Platform
	}{
		{"", types.MessengerLiteAndroid},
		{"android", types.MessengerLiteAndroid},
		{"Android", types.MessengerLiteAndroid},
		{" android ", types.MessengerLiteAndroid},
		{"ios", types.MessengerLiteIOS},
		{"iOS", types.MessengerLiteIOS},
	} {
		got, err := parsePlatform(tc.raw)
		if err != nil {
			t.Fatalf("parsePlatform(%q) failed: %v", tc.raw, err)
		}
		if got != tc.want {
			t.Fatalf("parsePlatform(%q) = %v, want %v", tc.raw, got, tc.want)
		}
	}
}

// TestPlatformRejectsUnknown keeps a typo from silently logging in as Android.
func TestPlatformRejectsUnknown(t *testing.T) {
	for _, raw := range []string{"windows", "messenger", "iosx", "1", "android,ios"} {
		got, err := parsePlatform(raw)
		if err == nil {
			t.Fatalf("parsePlatform(%q) = %v, want an error", raw, got)
		}
		if got != types.Unset {
			t.Fatalf("parsePlatform(%q) returned %v on the error path", raw, got)
		}
	}
	_, err := parsePlatform("windows")
	if err == nil {
		t.Fatal("expected an error")
	}
	for _, want := range []string{envPlatform, `"windows"`, platformAndroid, platformIOS} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("error %q does not name %q", err.Error(), want)
		}
	}
}

// TestProxyAcceptsSupportedSchemes covers every scheme the transport can dial.
func TestProxyAcceptsSupportedSchemes(t *testing.T) {
	for _, raw := range []string{
		"http://127.0.0.1:8080",
		"https://proxy.example:443",
		"socks5://127.0.0.1:1080",
		"socks5h://user:pass@127.0.0.1:1080",
		"HTTP://127.0.0.1:8080",
	} {
		u, err := parseProxy(raw)
		if err != nil {
			t.Fatalf("parseProxy(%q) failed: %v", raw, err)
		}
		if u == nil || u.Hostname() == "" {
			t.Fatalf("parseProxy(%q) = %v, want a URL with a host", raw, u)
		}
	}
}

// TestProxyRejectsUnsupportedValues fails at startup rather than at the first request.
func TestProxyRejectsUnsupportedValues(t *testing.T) {
	for _, raw := range []string{
		"ftp://127.0.0.1:21",
		"127.0.0.1:1080",
		"http://",
		"socks5://",
		"http:///no-host",
		"://host",
	} {
		if u, err := parseProxy(raw); err == nil {
			t.Fatalf("parseProxy(%q) = %v, want an error", raw, u)
		}
	}
	if _, err := parseProxy("ftp://127.0.0.1:21"); !strings.Contains(err.Error(), "ftp") {
		t.Fatalf("scheme error does not name the scheme: %v", err)
	}
	if _, err := parseProxy("http://"); !strings.Contains(err.Error(), "no host") {
		t.Fatalf("host error is not specific: %v", err)
	}
}

// TestProxyUnsetMeansDirect keeps an empty variable a direct connection.
func TestProxyUnsetMeansDirect(t *testing.T) {
	u, err := parseProxy("   ")
	if err != nil || u != nil {
		t.Fatalf("parseProxy(empty) = %v, %v; want nil, nil", u, err)
	}
	cfg, err := loadSettings(envReader(nil))
	if err != nil {
		t.Fatalf("loadSettings without environment failed: %v", err)
	}
	if cfg.proxy != nil || cfg.describeProxy() != "direct" {
		t.Fatalf("unset proxy described as %q, want direct", cfg.describeProxy())
	}
}

// TestProxyRedactedInLogs keeps the password out of the startup line and the error messages.
func TestProxyRedactedInLogs(t *testing.T) {
	const secret = "s3cret"
	const raw = "socks5://user:" + secret + "@127.0.0.1:1080"
	if got := redactProxy(raw); got != "socks5://***@127.0.0.1:1080" {
		t.Fatalf("redactProxy(%q) = %q", raw, got)
	}
	// A path after the authority survives redaction.
	if got := redactProxy("http://user:" + secret + "@proxy.local:3128/egress"); got != "http://***@proxy.local:3128/egress" {
		t.Fatalf("redactProxy with a path = %q", got)
	}
	// Text that never parsed as a URL is redacted too, and a value without userinfo is untouched.
	if got := redactProxy("user:" + secret + "@127.0.0.1:1080"); got != "***@127.0.0.1:1080" {
		t.Fatalf("redactProxy on schemeless userinfo = %q", got)
	}
	if got := redactProxy("http://127.0.0.1:8080"); got != "http://127.0.0.1:8080" {
		t.Fatalf("redactProxy stripped a URL without userinfo: %q", got)
	}

	cfg, err := loadSettings(envReader(map[string]string{envProxy: raw}))
	if err != nil {
		t.Fatalf("loadSettings(%q) failed: %v", raw, err)
	}
	if described := cfg.describeProxy(); strings.Contains(described, secret) || !strings.Contains(described, "127.0.0.1:1080") {
		t.Fatalf("startup description leaked credentials or lost the host: %q", described)
	}

	_, err = parseProxy("ftp://user:" + secret + "@127.0.0.1:21")
	if err == nil {
		t.Fatal("expected an error for the ftp scheme")
	}
	if strings.Contains(err.Error(), secret) {
		t.Fatalf("proxy error leaked the password: %v", err)
	}
}

// TestLoadSettingsReadsEnvironment checks both variables reach the settings together.
func TestLoadSettingsReadsEnvironment(t *testing.T) {
	cfg, err := loadSettings(envReader(map[string]string{
		envPlatform: "iOS",
		envProxy:    "http://127.0.0.1:3128",
	}))
	if err != nil {
		t.Fatalf("loadSettings failed: %v", err)
	}
	if cfg.platform != types.MessengerLiteIOS || cfg.platformName() != platformIOS {
		t.Fatalf("platform = %v (%q), want ios", cfg.platform, cfg.platformName())
	}
	if cfg.proxy == nil || cfg.proxy.Host != "127.0.0.1:3128" {
		t.Fatalf("proxy = %v, want 127.0.0.1:3128", cfg.proxy)
	}
	if cfg.config == nil || cfg.config.ClientSettings.ProxyAddress != "http://127.0.0.1:3128" {
		t.Fatalf("proxy did not reach the client settings: %+v", cfg.config)
	}
}

// TestPlatformSelectsClientPlatform proves the setting decides the platform of the clients the
// login builds, not just the cookie jar the sidecar keeps.
func TestPlatformSelectsClientPlatform(t *testing.T) {
	for _, tc := range []struct {
		raw  string
		want types.Platform
	}{
		{"", types.MessengerLiteAndroid},
		{"ios", types.MessengerLiteIOS},
	} {
		cfg, err := loadSettings(envReader(map[string]string{envPlatform: tc.raw}))
		if err != nil {
			t.Fatalf("loadSettings(%q) failed: %v", tc.raw, err)
		}
		login := newLiteLogin(cfg, zerolog.Nop())
		if got := login.cli.Platform; got != tc.want {
			t.Fatalf("platform %q built a client for %v, want %v", tc.raw, got, tc.want)
		}
		if got := login.cli.GetCookies().Platform; got != tc.want {
			t.Fatalf("platform %q built cookies for %v, want %v", tc.raw, got, tc.want)
		}
	}
}

// TestProxyReachesEveryClient proves the helper hands the proxy to the client it builds, so the
// login, restore, socket and media paths all share one transport configuration.
func TestProxyReachesEveryClient(t *testing.T) {
	cfg, err := loadSettings(envReader(map[string]string{envProxy: "socks5://127.0.0.1:1080"}))
	if err != nil {
		t.Fatalf("loadSettings failed: %v", err)
	}
	cli := cfg.newMessagixClient(zerolog.Nop(), &cookies.Cookies{Platform: types.MessengerLiteAndroid})
	http := cli.GetHTTP()
	if http == nil {
		t.Fatal("client has no HTTP client")
	}
	if http.GetNewProxy == nil {
		t.Fatal("client was built without the proxy provider")
	}
	addr, err := http.GetNewProxy("test")
	if err != nil || addr != "socks5://127.0.0.1:1080" {
		t.Fatalf("proxy provider = %q, %v", addr, err)
	}
	if !http.UpdateProxy("test") {
		t.Fatal("client rejected the configured proxy")
	}
	// The HTTP and websocket clients are compiled from these settings, so the proxy being here is
	// what keeps the socket off a direct connection.
	if http.HTTPSettings.ProxyAddress != "socks5://127.0.0.1:1080" {
		t.Fatalf("client settings proxy = %q", http.HTTPSettings.ProxyAddress)
	}

	direct := defaultSettings().newMessagixClient(zerolog.Nop(), &cookies.Cookies{Platform: types.MessengerLiteAndroid})
	if direct.GetHTTP().GetNewProxy != nil {
		t.Fatal("a direct connection installed a proxy provider")
	}
}

// TestSessionRestoresEitherLiteVariant keeps an iOS login restorable.
func TestSessionRestoresEitherLiteVariant(t *testing.T) {
	for _, want := range []types.Platform{types.MessengerLiteAndroid, types.MessengerLiteIOS} {
		got, err := sessionPlatform(want.String())
		if err != nil {
			t.Fatalf("sessionPlatform(%q) failed: %v", want.String(), err)
		}
		if got != want {
			t.Fatalf("sessionPlatform(%q) = %v, want %v", want.String(), got, want)
		}
	}
	if _, err := sessionPlatform("instagram"); err == nil {
		t.Fatal("expected an error for a foreign platform name")
	}
}
