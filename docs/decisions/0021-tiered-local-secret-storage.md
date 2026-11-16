---
id: adr-0021-tiered-local-secret-storage
type: architecture-design
status: proposed
title: "ADR 0021: Tiered local secret storage, UX first"
parent: alloy-architecture
depends-on:
  - adr-0019-desktop-secret-storage
  - adr-0020-server-primary-sync
tags: [adr, security, storage, keychain, android, ux]
---

# 0021: Tiered local secret storage, UX first

- Status: proposed (2026-11-16)
- Issue: #74 (`security`, open) — this ADR is the design half of it
- Date: 2026-11-16
- Extends: ADR 0019 (owner-only file on desktop, keychain opt-in)

## Context

ADR 0019 settled the desktop default: `<app data dir>/secrets.json` at 0600, keychain only through
`ALLOY_SECRET_BACKEND=keychain`, one platform read per process, no write probe. Android has shipped
the same file since the first release. #74 was deferred from #37: wrap that file's secrets with the
Android Keystore, and use `SecretString`/redacting `Debug` for passwords and recovery keys.

ADR 0020 adds a master key to that store, and states plainly that a client device with an attacker
running as the user is total compromise. What it cannot state is the trade the user is making,
because there is no policy yet for *how much friction buys how much protection*.

The maintainer's rule (2026-11-16): if a hardening step costs too much UX, take the weaker option.
So the design has to name the tiers and their prices, not one "secure" target.

Constraints inherited from the tree: workspace `forbid(unsafe_code)`, no hand-rolled crypto,
`chacha20poly1305` and `sha2` already present, secrets must survive an app restart without
re-derivation (ADR 0019's one-time adoption rule), and nothing may block the first window.

## Decision

Local secret storage is a **tier** the user sits in, each tier a complete configuration rather than a
flag on one mechanism. Tiers raise resistance to a stolen-disk or stolen-locked-device attacker;
none of them change what the sync server can see (ADR 0020 fixes that shape).

| Tier | Where secrets live | UX cost | Stands against |
|---|---|---|---|
| **0 — file** (default everywhere today) | `secrets.json` 0600 | none: no prompt, ever | other users, offline disk, backups taken without the file |
| **1 — hardware wrap** (default on Android once #74 lands; opt-in on desktop where the keychain behaves) | tier 0 file holds ciphertext; the unwrapping key is Keystore/keychain-held, released after the device credential or biometric | one OS unlock per app launch at most, never per message | disk image, backup extraction, other accounts — not a running attacker |
| **2 — passphrase** (opt-in) | as tier 0, plus the master key and DB key wrapped with `argon2id(passphrase)` | one passphrase entry per launch, or per unlock timeout the user picks | the same, plus an attacker who has the disk *and* the OS login |

The tier is user-visible in Settings → Security, with the price of the *next* tier written next to
its button. Downgrading is allowed (that is the rule), and it is never silent: dropping to tier 0
warns once with the concrete exposure, e.g. *"a stolen laptop disk can read your saved tags, groups and
theme until you revoke the device."*

### Rules that keep this from becoming a nag

- **One prompt per launch, maximum.** Nothing in this ADR may add a prompt to message send, sync, or
  window focus. A prompt that recurs is a UX failure no matter what it protects.
- **Never block the first paint.** Tier 1/2 start the UI in a locked state that renders the shell;
  secrets resolve on the first operation that needs them.
- **Locked means deferred, not lost.** Background sync pauses while locked; it resumes on unlock.
  Nothing is queued into a plaintext buffer.
- **Tier 0 stays the default** until a tier is measured to be prompt-free on that platform. A tier
  that prompts on every launch on a platform (the ADR 0019 macOS finding) is not offered there.
- **Adoption is one-way and cheap to keep.** A machine already running tier 0 adopts an added tier in
  one migration; a missing wrap key is a locked state, never a silently minted new key that would make
  the existing store unreadable.

### Crypto and format

- Wrapping uses the same audited primitives as `alloy-sync`: `argon2id` for passphrases (parameters
  stored beside the blob, versioned), `ChaCha20-Poly1305` for the wrap, `zeroize` on every decrypted
  buffer, `secrecy`'s `SecretString` at the API boundary so no key reaches a `Debug` or a log line.
- Android Keystore: `AES-GCM` key, `setUserAuthenticationRequired(true)` only when the device has a
  secure lock screen, `setInvalidatedByBiometricEnrollment(true)` so adding a fingerprint locks the
  store, and a documented "keystore key invalidated" path that asks for the tier's fallback rather
  than minting a new key over an existing database.
- Every wrap blob carries `{version, kdf, params, salt, nonce}`, and a failed unwrap is a typed error
  that names the recovery path, not a generic "decryption failed".
- No new crypto: #74's `SecretString`/redacting `Debug` work lands first, since a key that leaks into
  a log is not protected by any tier.

## Alternatives considered

- **Make the passphrase mandatory (tier 2 everywhere).** Rejected: it is the highest-friction option
  on every launch of a chat client, and the maintainer's rule rejects it outright.
- **Keychain as the desktop default (revisited).** Rejected in ADR 0019 and still rejected: macOS
  re-prompts per binary identity, which is per build.
- **Per-use biometrics (prompt on every unlock of a chat).** Rejected: latency in the primary loop.
- **Encrypt the whole database file at rest instead of wrapping keys.** Rejected: it is the same
  product experience and strictly more work; SQLCipher already encrypts the database, the problem is
  the key sitting beside it.
- **No tiers — one target and a settings switch.** Rejected: it hides the price. Tiers make the
  tradeoff legible and let Android take the free win (Keystore) while desktop stays prompt-free.

## Consequences

- `alloy-storage` gains a `SecretStore` tier trait with three implementations and one migration path;
  `secrets.json` stays readable by every tier so a downgrade is not a data loss event.
- #74 becomes the implementation issue for tier 1 (Android), plus the `SecretString` boundary. Tier 2
  is its own slice; tier 0 needs no work beyond a lock-state surface in the UI.
- Testing: a locked-state suite (render shell while locked, unlock, resume sync), a prompt-count
  assertion (tier 0 and tier 1 issue zero app-owned prompts per launch on the CI platforms), and an
  invalidated-keystore test that must reach the recovery path, not a fresh key.
- ADR 0020's "locked device" row becomes accurate only on platforms where tier 1 or 2 is available and
  enabled; the ADR text must not claim more than the running tier delivers.
