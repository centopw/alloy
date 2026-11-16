---
id: adr-0020-server-primary-sync
type: architecture-design
status: proposed
title: "ADR 0020: Optional zero-knowledge sync server as primary store"
parent: alloy-architecture
depends-on:
  - adr-0016-on-device-connectors
  - adr-0019-desktop-secret-storage
tags: [adr, architecture, sync, security, crypto, hosting]
---

# 0020: Optional zero-knowledge sync server as primary store

- Status: proposed (2026-11-16)
- Issue: none yet (file one with `type:decision` when the board is updated)
- Date: 2026-11-16
- Amends: ADR 0016's "no homeserver" — the server below is optional *storage*, never an identity
  provider; there is still no Matrix, no OIDC, and no server-side login to the account.

## Context

Alloy today is single-device by construction. The account's connector material lives in
`account_secrets` inside the SQLCipher database, the database key lives in `secrets.json` (0600, or
the keychain when `ALLOY_SECRET_BACKEND=keychain` — ADR 0019), and nothing leaves the machine. Moving
to a second device means repeating onboarding: reconnect Telegram/Messenger, re-create themes, tags,
groups, pins, and layout.

Two separate needs get confused in that complaint:

1. **Enrolment** — getting a second device onto the same *configuration* without retyping it.
2. **Continuity of storage** — one place that holds the data so a device can be a thin client.

They have different threat models, so they are different transports. Enrolment is device-to-device
and needs no infrastructure at all. Storage needs a server, and a server holding a personal
messenger account's data is exactly the thing that must be designed so that its compromise is
boring.

The rule that survives ADR 0016: Alloy must be fully functional with no server configured. The server
is additive. Local-only is not a degraded mode; it is the default.

## Decision

Add an **optional, zero-knowledge sync server** that may act as the primary store for Alloy's own
state, with devices as clients. Server compromise yields no plaintext and no account takeover, and a
device that never configures one behaves exactly as today.

Three mechanisms carry the whole design:

**1. Enrolment is device-to-device, never server-mediated.** An existing device mints a QR payload
`{pairing_id, ephemeral_x25519_pubkey, lan_hints[]}`; the new device shows it, the existing device
scans it, both display a 6-digit SAS, and the existing device sends the sealed bundle (the master
key, the device roster, and the sync endpoint/token if one is configured) over the LAN or a blind
relay. When no camera is available the QR is replaced by an 8-character code and the same ephemeral
exchange. **The server can never enrol a device** — the trust root is a device that already exists.

**2. Keys never reach the server.** The first device generates a 32-byte master key. Everything else
is derived:

| Material | Derivation |
|---|---|
| Blob AEAD key | `HKDF(master, info="blob")` |
| Blob address key | `HKDF(master, info="addr")` |
| Device-roster signing key | `HKDF(master, info="dev")` |
| Recovery wrapper | `argon2id(24-word phrase) → AEAD(master)` |

Blob identifiers are `HMAC(addr_key, logical_id)`, so the server cannot enumerate, correlate, or name
anything it stores. Blob bodies are `ChaCha20-Poly1305`, padded to a fixed size class. Each device
holds its own Ed25519 identity; enrolment countersigns it into the roster.

**3. History is a signed hash chain the server cannot forge.** Every uploaded batch carries
`{device_id, seq, prev_head, root_hash, signature}`. The server enforces compare-and-swap on `head`
(one writer wins, the loser rebases), and the *client* verifies the chain and every signature. A
malicious or buggy server can refuse to serve; it cannot rewrite, fork, or roll back without the
device noticing on the next `head` read.

### Store layers

"Server is primary" is scoped per layer so the server does not become a duplicate of Telegram's and
Messenger's own clouds:

| Layer | Default with a server | Why |
|---|---|---|
| Settings, theme, wallpaper, keybinds, layout | mirror on server | pure client state, nothing else holds it |
| Tags, groups, pins, read markers, drafts, notes | mirror on server | Alloy-authored, unrecoverable if lost |
| Connector session material (`account_secrets`) | **never** leaves the device | cloning an auth key makes two clients one session and splits updates; each device logs in with the connector's own QR login |
| Message bodies, media | off; opt-in per account | connectors already re-sync history; mirroring it duplicates a corpus the user does not control us to keep |

A fresh device enrols, unseals the master key, pulls the layers the user enabled, and populates its
local SQLCipher database. The local database stays the working set — the server is authoritative for
what it stores, not an RPC endpoint the UI waits on. Offline behaviour is unchanged.

### Recovery

Server-as-primary means losing every device is losing the configuration. Two paths, user's choice at
setup:

- **24-word phrase** (recommended, off by default): the phrase wraps the master key into an escrow
  blob on the server. A fresh device with only the phrase can recover. Argon2id parameters are
  stored beside the blob and validated before use.
- **No phrase**: recovery requires enrolling from a device that still exists. The server keeps
  ciphertext it can never open again; it is offered for deletion and the UI says so plainly.

### Hosting

| Mode | Server holds | Fits |
|---|---|---|
| `off` (default) | nothing | any build |
| `prefs` | encrypted config/tag/group blobs, `head` CAS | Cloudflare Workers + KV or D1, Vercel + KV, a $0 tier, or Docker |
| `full` | the above plus opt-in message/media mirror | self-hosted Docker, or R2 + Durable Objects; not Vercel/Workers — serverless has no process for incremental push and free-tier KV is the wrong store for a growing archive |

The wire contract is one small HTTP API (blob `GET`/`PUT` under a `head` CAS, roster `GET`, escrow
`GET`/`PUT`), so a self-hosted deployment is a container the user runs, and the hosted options are
the same contract behind a managed KV.

### Threat model, as acceptance criteria

| # | Invariant | Verified by |
|---|---|---|
| 1 | Server holds no key that decrypts content, no plaintext | blob bodies are AEAD output only; grep-level check on the server crate's types |
| 2 | Connector session material never uploaded | upload allow-list keyed by table name, not by caller |
| 3 | Server cannot enrol a device | enrolment path takes no server input |
| 4 | Server cannot fork or roll back silently | head-chain verification test with a replaying/mutating fake server |
| 5 | Server cannot enumerate or correlate blobs | `HMAC` ids; no plaintext identifiers in keys or bodies |
| 6 | Server loss is not data loss for a configured device | full read-only operation with the server stopped |
| 7 | Only audited primitives | `chacha20poly1305` and `sha2` already in `alloy-store`; add `x25519-dalek`, `ed25519-dalek`, `hkdf`, `argon2`, `zeroize`; no hand-rolled crypto |
| 8 | Keys at rest obey ADR 0019 | master key in `secrets.json` (0600) or keychain; zeroized on drop |
| 9 | Metadata does not leak | TLS 1.3 only, no plaintext headers beyond auth, padded body sizes, server access logging off by default |
| 10 | AEAD is bound to identity and order | AAD = `(layer, logical_id, seq, epoch)`, so a replay, a swap, or a relabel fails to decrypt |
| 11 | Client trusts no server metadata for correctness | head is monotonic per device; a fork is detected on the next read and on enrolment |
| 12 | The sync endpoint is pinned | enrolment payload carries the endpoint's SPKI hash; a changed pin blocks sync and warns |
| 13 | Untrusted server input is bounded | max blob size, padding size classes only, max chain length, no decompression on a server-supplied field |
| 14 | Revocation rotates keys | removing a device bumps the epoch and re-seals the config layers; the server drops its token |

Invariants 1–6 are tests against an in-repo fake server, not claims. A test that only exercises the
honest server is not evidence.

## Adversary: root on the server

Assume the operator can read the disk, rewrite the code, serve arbitrary responses, drop or replay
anything, and log every request. Cheap VPS hosting makes this the *expected* case, not the exotic
one.

| Attack with server root | Result |
|---|---|
| Read the database | ciphertext, padded sizes, `HMAC` blob ids, head hashes. No plaintext, no key, no plaintext identifier |
| Serve a rewritten or forked history | signature failure on the batch, or a head the client has already passed: refuse, warn, keep local state |
| Replay an old blob into a new slot | decryption fails (invariant 10) |
| Impersonate the server to a client | pinned SPKI mismatch; and even a successful MITM learns ciphertext |
| Serve garbage of unbounded size | rejected by size/chain bounds before any decrypt (invariant 13) |
| Read the relay used for cross-network enrolment | the envelope is sealed to the new device's ephemeral key; the relay sees opaque bytes and a pairing id |
| MITM the enrolment rendezvous | both devices display a SAS derived from both ephemeral keys; a mismatch shows different digits and the user stops |
| Enrol its own device into the account | impossible — enrolment needs a QR shown by an existing device and a SAS confirmation there. The server is never in that path |
| Deny service, delete everything | availability loss only; each configured device still runs on local state (invariant 6) |

Residual risk that cannot be engineered away: **a compromised server learns traffic metadata**
(when, how much, which device, roughly which layer by size class). Padding reduces it; it does not
remove it.

## Adversary: the client device in the wrong hands

Separate the two cases, because only one of them is defensible.

**A stolen, locked device** (attacker has the hardware, not the user's session). Controls: the master
key and the SQLCipher key live behind the platform's own lock — hardware-backed wrapping on Android
(Keystore) and, where available, the OS keychain on desktop; an optional passphrase wraps the master
key so a disk image alone does not yield it. This is the case the design is expected to survive.

**A stolen or compromised device with the attacker running as the user** — shell access, or any
malware in the user's session. Honest answer: **this is total compromise of that device.** A
same-UID process can read `secrets.json` (0600 is not a defence against its owner), the SQLCipher
database while unlocked, and the connector session material. No client-side crypto fixes that. What
the design does instead is bound the damage:

| Control | Effect |
|---|---|
| Signed device roster | the stolen device is one named entry, and the user can see it, with last-seen and last-sync |
| Revocation bumps the epoch | after revocation the stolen device cannot decrypt *new* config, even though it keeps the old master key |
| Re-seal on rotation | config layers are re-sealed under the new epoch on the remaining devices; the old ciphertext on the server is deleted, not just orphaned |
| Remote wipe flag | on next contact the device erases local state — best effort, worthless against an attacker who stays offline, so it is advertised as best effort |
| Enrolment requires a live existing device | a stolen device can enrol its own client only while it is unlocked and in the user's hands; rotation after revoke is the remedy |
| Rotation is visible and cheap | "revoke this device" in Settings rotates the config-key epoch in the background; the user is told history mirroring is not retroactively protected |

The uncomfortable consequence, stated in the UI rather than the fine print: **a device that ran with
the master key keeps everything it already saw.** Rotation protects the future, never the past. This
is also why the recovery phrase and the master key must not be treated as replayable credentials for
long-lived secrets.

Follow-up this ADR does not cover: hardening local secret storage itself (Android Keystore wrapping,
issue #74, and a desktop passphrase gate) is a prerequisite for the "stolen locked device" row above
on platforms without hardware-backed storage. It should be its own ADR.

## Alternatives considered

- **Copy `account_secrets` to the new device.** Rejected: sharing a Telegram auth key makes both
  clients one session — updates, read markers and typing split unpredictably, and the server that
  carried it could have held the account. Enrolment transfers configuration; each device signs in
  with the connector's own QR login.
- **Server-mediated enrolment (log in to the server, pull the key).** Rejected: it makes the server
  a second trust root, so server compromise becomes account compromise — invariant 2 fails by
  construction.
- **Plaintext or server-decryptable storage.** Rejected: a self-hosted server is often a VPS someone
  else operates; "we trust the operator" is not a standard, and it would make the hosted tiers
  indefensible.
- **Server as mirror only.** Rejected by product decision (2026-11-16): the user wants a device that
  starts empty and pulls its world. Mirror mode survives as the `prefs` hosting tier.
- **Real-time CRDT sync as the first step.** Rejected: config is LWW per entity; messages are
  append-only. A hash chain plus per-domain merge rules covers it, and CRDTs would be an abstraction
  paid for before it is needed.

## Consequences

- New crate `crates/alloy-sync` (protocol, key hierarchy, blob codec, head chain) plus a thin client
  in `alloy-engine`; the server itself ships as a separate, small binary/repo so the Apache-2.0
  client never links server code.
- `alloy-storage` gains export/import of the configuration layers — usable by enrolment with no
  server at all, which is the first shippable slice.
- Settings UI gains: "Sync server" (off / own URL + token), "What the server holds" per layer, and
  recovery-phrase setup/verify.
- A device with no server configured must pass the full suite unchanged; that remains the guard
  against this ADR quietly becoming mandatory infrastructure.
- Migration: an existing single device becomes the first writer; its first upload seeds `head`, so
  nothing is re-derived or rewritten.

## Slices

| Slice | Deliverable | Proof |
|---|---|---|
| A | `alloy-storage` config bundle export/import + QR / 8-char pairing over LAN, no server | two machines exchange settings, theme, tags, groups; no network egress besides the LAN pair |
| B | `crates/alloy-sync`: key hierarchy, blob codec with AAD binding, head chain, endpoint pinning; fake server in-repo | invariants 1, 2, 4, 5, 10, 11, 12 as tests against the hostile fake |
| C | `prefs` server (Workers-compatible contract) + client sync loop | fresh device enrols and pulls config; server stopped → app unchanged |
| D | Device roster, revocation, and epoch rotation with re-seal | revoke a device, assert it cannot decrypt the next config blob; remaining devices converge |
| E | Recovery phrase + escrow | recover on a clean device from the phrase alone |
| F | Opt-in message/media mirror (`full`) | gated on B's hostile-server suite; documented as self-host/R2 only |

Slice F is the only one that enlarges the blast radius of a stolen device, which is why it comes
after D rather than with B.
