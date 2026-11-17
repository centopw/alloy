---
id: adr-0022-on-demand-chrome-session-import
type: architecture-design
status: accepted
title: "ADR 0022: Messenger session imported from Chrome on demand, once"
parent: alloy-architecture
depends-on:
  - adr-0016-on-device-connectors
tags: [adr, messenger, onboarding, keychain, chrome, ux]
---

# 0022: Messenger session imported from Chrome on demand, once

- Status: accepted (2026-11-17)
- Issue: B1 (chrome-session)
- Date: 2026-11-17
- Applies to: `apps/alloy/src-tauri/src/chrome.rs`, the Messenger cookie step in `LoginStep.svelte`

## Context

Linking a Messenger account needs the browser session's cookies (`xs`, `c_user`, `datr`, plus `sb`
and `fr` when present). Until now the person copied a request out of DevTools by hand.

Chrome already holds that session. Reading it is cheap in code and expensive in trust: on macOS the
cookies are AES-encrypted with a key in the login Keychain ("Chrome Safe Storage"), so the first read
raises a system prompt that names Alloy and asks for the login password. A Telegram-only person must
never meet that prompt, a profile picker, or any Chrome file access — not at launch, not on the
settings page. That is a hard requirement, not a preference.

The session also has to be enough on its own. Once linked, the account must keep working when Chrome
closes, the profile changes, or the browser rotates its cookies.

## Decision

**The Chrome profile is read only when the person presses "Use the Chrome session on this Mac" in the
Messenger cookie step, and exactly once per account.**

### What is read, and what the prompt covers

- The command (`import_chrome_session`) resolves `~/Library/Application Support/Google/Chrome`,
  finds each profile folder (`Default`, `Profile *`) that has a cookie store (`Network/Cookies`, or
  `Cookies` on a profile Chrome has not migrated), copies that file and its `-wal`/`-shm` sidecars
  into a 0700 temporary directory as 0600 files, and reads the copy read-only. Chrome itself is never
  opened for writing and its lock is not disturbed.
- Rows are filtered before any secret is touched: only the five cookie names the Messenger cookie
  login accepts, only hosts under `messenger.com` (preferred) or `facebook.com`, one value per name.
- A profile counts as a session only when `xs` and `c_user` are both present. Zero such profiles is
  "not signed in"; more than one is an error naming the profiles, because Alloy cannot guess which
  account the person meant.
- Only then is the Keychain item read: service `Chrome Safe Storage`, account `Chrome`. The system
  prompt is the Keychain's own "Alloy wants to access Chrome's key", so it covers exactly that one
  item. Key derivation is Chrome's published scheme: PBKDF2-HMAC-SHA1, salt `saltysalt`, 1003
  iterations, 16 bytes; AES-128-CBC with a sixteen-space IV; `v10` prefix stripped; Chrome's
  SHA-256(host) prefix removed when the build writes one.
- The temporary copies are deleted when the read finishes, whatever the outcome. Cookie values never
  appear in a log line, an error message, or the disk: errors name the failure only.

### Nothing runs earlier

`ChromeProfile` holds a root path and a Keychain handle; constructing it performs no I/O, and the
reader is built inside the command and dropped after it. No app state retains it, so app startup and
opening settings have no path to Chrome or the Keychain. A unit test proves the shape: construction
and a read against a missing profile touch the Keychain zero times, and a successful read touches it
once. The UI offers the action only in the desktop Mac build (`isChromeSessionSupported`), so the web
build and phone builds show nothing about Chrome.

### Once imported, Chrome is out of the picture

The imported cookies are submitted as the ordinary cookie login. The sidecar validates them by
loading the inbox, returns its session blob, and the engine stores that blob in the account's own
session material (`put_secret`, the same place a hand-pasted cookie login lands). It survives a
restart because it is read back on the next launch.

From there the sidecar owns the session: messagix updates its cookie jar from every response's
`Set-Cookie` (`cookies.UpdateFromResponse`), and the sidecar's `sessionRotationLoop` re-encodes the
blob every two minutes and emits `SessionUpdated`, which the engine persists. Alloy never reads
Chrome again for that account — not on launch, not on reconnect, not on renewal.

The only thing that may ask the person again is Meta invalidating the session (signed out, password
change, device revoked). That surfaces as the ordinary "sign in again" state; the Chrome action is
offered there as one option among the same alternatives, never as an automatic re-read.

## Alternatives considered

- **Read Chrome at startup and cache the cookies.** Rejected: it prompts every Telegram-only person,
  which is the one thing this ADR exists to prevent.
- **A profile picker before import.** Rejected: it is Chrome access by another name. Several complete
  sessions are an error that names the profiles, not a UI that probes them.
- **Keep the cookies in Alloy's own config so the connector can re-import later.** Rejected: the
  session blob the connector stores is already the renewal-capable copy, and a second stored copy of
  browser cookies would be a second secret to protect for no gain.
- **Read Chrome again when the session dies.** Rejected: it hides Meta's revocation from the person
  and re-raises the Keychain prompt at an arbitrary time.
- **Ask Chrome for the signed-in account name.** The numeric id is in `c_user`, but a name would need
  a network request to messenger.com; the flow says "session imported" instead.

## Consequences

- One Tauri command, one platform wrapper, one action in the Messenger cookie step; no other surface
  mentions Chrome.
- Failure messages are distinct and actionable: Chrome not installed, profile not signed in, more
  than one profile with a session, cookies incomplete (names the missing ones), Keychain refused,
  cookie file unreadable, unknown encryption scheme, undecryptable value. None of them falls back to
  reading anything else; the manual paste stays available.
- The scheme is verified against the Chrome installed on this machine (157.0.8103.79): `v10` rows,
  Keychain "Chrome Safe Storage", the KDF and IV above, and the SHA-256(host) prefix all reproduce
  real cookie values. A future Chrome that changes the scheme fails as `UnknownScheme` and says so
  instead of returning garbage.
- Tests: a pinned ciphertext made outside this code (fixed key, known plaintext) fails if the KDF,
  IV, or prefix handling moves; the selection filter is tested against decoy hosts, decoy names, a
  `facebook.com` duplicate, an extra profile, an incomplete profile and an expired session; the lazy
  rule is tested through the Keychain seam.
