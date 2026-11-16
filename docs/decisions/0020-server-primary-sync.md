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

Invariants 1–6 are tests against an in-repo fake server, not claims. A test that only exercises the
honest server is not evidence.

### Hosting

| Mode | Server holds | Fits |
|---|---|---|
| `off` (default) | nothing | any build |
| `prefs` | encrypted config/tag/group blobs, `head` CAS | Cloudflare Workers + KV or D1, Vercel + KV, a $0 tier, or Docker |
| `full` | the above plus opt-in message/media mirror | self-hosted Docker, or R2 + Durable Objects; not Vercel/Workers — serverless has no process for incremental push and free-tier KV is the wrong store for a growing archive |

The wire contract is one small HTTP API (blob `GET`/`PUT` under a `head` CAS, roster `GET`, escrow
`GET`/`PUT`), so a self-hosted deployment is a container the user runs, and the hosted options are
the same contract behind a managed KV.

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
| B | `crates/alloy-sync`: key hierarchy, blob codec, head chain; fake server in-repo | invariants 1, 2, 4, 5 as tests against the hostile fake |
| C | `prefs` server (Workers-compatible contract) + client sync loop | fresh device enrols and pulls config; server stopped → app unchanged |
| D | Recovery phrase + escrow | recover on a clean device from the phrase alone |
| E | Opt-in message/media mirror (`full`) | gated on B's hostile-server suite; documented as self-host/R2 only |
