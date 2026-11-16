---
id: adr-0019-desktop-secret-storage
type: architecture-design
status: accepted
title: "ADR 0019: Owner-only secret file on desktop, keychain opt-in"
parent: alloy-architecture
depends-on:
  - adr-0016-on-device-connectors
tags: [adr, storage, security, keychain, desktop]
---

# 0019: Owner-only secret file on desktop, keychain opt-in

- Status: accepted (2026-11-16)
- Issue: none yet (file one with `type:decision` when the board is updated)
- Date: 2026-11-16
- Supersedes: the startup rule that every desktop build opens the OS keychain

## Context

The engine keeps two secrets in the platform secret store: the SQLCipher key for `alloy.db` and the
user's Telegram application pair. Desktop builds opened the OS keychain at startup, probed it with a
write, and read the database key from it on every launch.

On macOS a keychain item's access control list is bound to the code signature of the binary that
created it. Every unsigned development build, and every release, is a different identity, so the
system asks for the login password again on each launch — a prompt the app cannot suppress and the
user cannot make permanent. The write probe made it worse by touching the keychain three times before
the first window appeared.

The prompt protects the database key from *other users* and from processes that cannot read the
user's home directory. On a single-user machine with full-disk encryption, that is the same
protection class the owner-only file already gives, and the Android build has shipped with exactly
that file since the first release.

## Decision

Desktop default is the owner-only secret file: `<app data dir>/secrets.json` (0600, atomic
write-and-rename, inside a directory the OS keeps private to the user). The OS keychain is opt-in
through `ALLOY_SECRET_BACKEND=keychain`.

Three supporting rules keep both backends cheap and honest:

- **One-time adoption.** On first start the file backend copies any key it lacks out of the keychain
  (store key, then Telegram pair), then never reads the keychain again. A key that is still missing
  while `alloy.db` exists aborts startup with the keychain error: minting a fresh key would make the
  existing database unreadable.
- **No write probe.** Opening a keychain store is a single read of an entry the app never writes; a
  missing entry proves the backend answers without prompting. The old set/get/delete probe is gone.
- **One read per process.** The keychain store caches every resolved key (value *or* absence), so a
  caller that reads the same secret repeatedly — the factory reads the Telegram pair on every
  `ListNetworks`/`BeginLogin` — never triggers a second platform read.

## Alternatives considered

- **Keep the keychain as the desktop default.** Rejected: on macOS it cannot be made prompt-free for
  a build whose signature changes, which is every local build and every release.
- **Sign every local build with a stable identity.** Rejected for now: it needs a developer
  certificate in the toolchain, does nothing for release-to-release prompts, and does not help
  Linux/Windows users at all.
- **Encrypt the file with a key derived from a user passphrase.** Rejected: it turns every launch
  into a password prompt, which is the problem being solved.
- **Windows DPAPI / Android Keystore wrapping.** Not adopted yet; the file store is the documented
  fallback and wrapping it is a later hardening step, not a precondition.

## Consequences

- No keychain prompt on launch, for any desktop build. The one prompt an existing install can see is
  the adoption read on the first start after upgrading, and it happens once.
- The database key now lives in a user-readable file. It is readable by any process running as that
  user — the same exposure the Android build already accepts. Losing the file loses the database.
- `ALLOY_SECRET_BACKEND=keychain` stays supported and now reads each secret at most once per process.
- Follow-up: wrap the file store with Android Keystore / Windows DPAPI, and file the `type:decision`
  issue for this record.
