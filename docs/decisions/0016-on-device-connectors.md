---
id: adr-0016-on-device-connectors
type: architecture-design
status: accepted
title: "ADR 0016: Replace Matrix with on-device connectors"
parent: alloy-architecture
depends-on:
  - adr-0006-shared-controls-tags
  - adr-0008-chat-parity
  - adr-0009-android-push
tags: [adr, architecture, connectors, telegram, messenger, storage]
---

# 0016: Replace Matrix with on-device connectors

- Status: accepted (2026-11-12)
- Issue: #126 (`type:decision`), epic #125
- Date: 2026-11-12
- Supersedes: D3; the Matrix parts of D15; ADR 0008's "ordinary Matrix mechanism" rationale; ADR 0009's Matrix pusher

## Context

Matrix was never the product. It was the transport that let the client reach a user's personal
Telegram and Messenger accounts through external bridges: network → bridge → homeserver → client.
Every conversation therefore needed a homeserver, a Postgres database, and one bridge per network,
which is the dominant cost of running Alloy. Synapse's official installation floor is 1 GB of RAM
for the homeserver alone, and Element's server-suite guidance is 2 GB for Synapse plus 4 GB for
Postgres for up to 500 users — before any bridge, media store or reverse proxy. None of that serves
the user, who only asked to read their own messages.

The Matrix mapping is also the dominant cost inside the client. About 60 of the 104 `EngineCommand`
variants exist only to express Matrix concepts, and all of `alloy-core` (about 7.4k LOC) and all of
`alloy-push` are Matrix-specific. Every feature was paid for twice: once in the Matrix projection,
once in the bridge that had to fake a Matrix room for a Telegram chat. The extra hop adds latency,
duplicated media, a second failure domain and a Matrix identity the user never asked for. Element
interoperability was a bet, not a requirement.

The networks themselves do not offer a personal-account API: Telegram ships MTProto and expects
third-party clients to implement it; Messenger is reached through its web/GraphQL front end. Both
are protocols the client has to speak directly if there is to be no server in the middle.

## Decision

Remove Matrix completely. The client connects personal Telegram and Messenger accounts directly
from the device through **connectors**, and stores every byte locally.

### 1. Connectors run on the device

- **Telegram** is in-process on [grammers](https://codeberg.org/Lonami/grammers), a pure-Rust
  MTProto implementation (MIT/Apache-2.0). It keeps the workspace's `forbid(unsafe_code)` intact,
  has no C++ toolchain, and builds for the same five targets as the app (macOS arm64/x64, Windows,
  Linux, Android).
- **Messenger** is a Go sidecar, `sidecars/alloy-meta`, built on mautrix-meta's `messagix`
  ([mautrix/meta](https://github.com/mautrix/meta)). A Rust `SidecarConnector` supervises the
  process and speaks NDJSON RPC over stdio. The sidecar is AGPL-3.0 and ships as a separate
  executable, never linked into Alloy's Apache-2.0 code.

The engine owns one actor per account and an outbox worker; connectors are driven through a
serializable trait, not through Matrix sync.

### 2. Every byte is stored locally

`alloy-store` is a local SQLite database with versioned migrations (schema v1 in M0). Secret
material — sessions, tokens, cookies — is opaque bytes produced by a connector, encrypted at rest
by the store with a key held in the OS keyring through `alloy-storage`'s `SecretStore`. Connectors
never see the database.

### 3. The connector contract is serializable

`alloy-connector` defines the trait the engine drives. Everything crossing it is serializable so
the same connectors can later be hosted by the optional `alloy-gateway` (M6) without a second
implementation:

- Network names on the wire and in SQLite: `"telegram"`, `"messenger"` (lowercase; tests may use
  `"fake"`).
- `account_id` is a locally generated opaque TEXT (UUID v4 string). Remote ids
  (`remote_chat_id`, `remote_msg_id`, `remote_user_id`) are opaque TEXT exactly as the network
  gives them.
- A local message id is a UUID string; `txn_id` is a client-generated UUID string used to
  reconcile the outbox with the remote echo.
- Timestamps are i64 milliseconds since the Unix epoch, UTC.
- `alloy-store` and `alloy-connector` do not depend on each other; the engine maps between them.

### 4. A gateway is optional, not the architecture

M6 may host the same connectors server-side on Postgres. Clients then sync from a monotonic
`event_log.seq` over WSS instead of running the connector themselves. Push becomes possible again
only on that path.

## Alternatives considered

- **Keep Matrix and the bridges.** The homeserver, Postgres and bridge stack is the dominant cost;
  the client still needs two hops for every message; and roughly 60% of the engine's command
  surface exists only to fake Matrix semantics for networks that do not have them. Rejected.
- **Self-hosted gateway only.** One server per user still exists, so "local data on the client"
  fails, every user must run infrastructure, and the cost merely moves from the maintainer to the
  user. Rejected.
- **TDLib for Telegram.** The most complete Telegram implementation, but a C++ FFI build on five
  targets, an unavoidable `unsafe` boundary that breaks `forbid(unsafe_code)`, and a much larger
  binary. Rejected in favour of grammers; TDLib remains the fallback if grammers blocks a required
  feature (secret chats, some media paths).
- **Rewrite the Messenger protocol in Rust.** Reverse-engineering Facebook's private front end is a
  permanent reimplementation project with no licence clarity. Reusing mautrix-meta's `messagix` in a
  separate process is the smaller, better-tested surface. Rejected.
- **Adopt the Beeper bridge stack.** It solves the network side but keeps Matrix, a homeserver and
  an external service in the path — the three things this decision removes. Rejected.

## Consequences

- **D3 is superseded.** The Matrix engine row in `PLAN.md` stays only as history.
- **D15 is superseded where it is Matrix-specific.** Custom room tags stop being Matrix `u.<id>`
  room tags and become rows in `chats.local_tags`; the tag catalogue and UI behaviour stay.
- **ADR 0008's rationale is superseded.** "Every feature is an ordinary Matrix mechanism, so
  Element sees the same rooms" is no longer true; features map to per-network capabilities instead,
  and parity with Telegram/Messenger replaces parity with Element.
- **ADR 0009's Matrix pusher is superseded.** On Android a foreground service keeps the connectors
  alive; there is no homeserver to register a pusher with. Server push exists only with the M6
  gateway.
- **UI removal in M4:** E2EE device verification and recovery, homeserver discovery, spaces, room
  directory and power levels have no meaning without Matrix and are deleted with the Matrix path of
  `alloy-push`, the Matrix-only protocol variants and `alloy-core`.
- **No Element interop.** Alloy users can no longer share a room with Element users; the product is
  a client for two proprietary networks, not a Matrix client.
- **Risks accepted:**
  - Telegram's API terms put unofficial clients under observation; the client must ship with its
    own `api_id` and never suppress read receipts or typing indicators (API terms §1.4:
    [terms](https://core.telegram.org/api/terms), [api_id](https://core.telegram.org/api/obtaining_api_id)).
  - Meta has no personal-account API. Account locks are documented in the wild
    ([mautrix/meta#44](https://github.com/mautrix/meta/issues/44)); the ban risk must be shown to the
    user before connecting Messenger.
  - The sidecar is AGPL-3.0. It stays a separate executable behind NDJSON; a licence review gate
    must pass before release.
  - Messenger's private protocol changes without notice; the sidecar needs a pinned, updatable
    `messagix` version and a recovery path.
  - Android background execution without a server depends on a foreground service, which costs
    battery and can be killed by OEM battery managers.

## Roadmap

| Milestone | Issues | Scope | Exit |
|---|---|---|---|
| M0 contracts | #126, #127, #128 | ADR 0016; `alloy-store` schema v1 migrations; `alloy-connector` contract | a fresh database migrates forward and refuses edited or newer schemas; the contract round-trips through serde |
| M1 engine and UI | #129, #130 | engine on `alloy-store` + connectors; one tokio actor per account; outbox worker; store → `VectorDiff` projections; protocol v2 (neutral ids, interactive login steps, per-network capabilities); "Connect account" onboarding | a fake connector runs end-to-end in the app: connect, list chats, send, receive, persist across restart |
| M2 Telegram | #131 | `crates/alloy-telegram` on grammers: phone/code/2FA and QR login, dialogs, backfill, send/edit/delete/react/read/typing, media, update-gap recovery, FLOOD_WAIT honoured | a personal Telegram account works end-to-end on desktop and Android without suppressing read/typing |
| M3 Messenger | #132 | `sidecars/alloy-meta` on `messagix` + `SidecarConnector`: cookie and password login, thread sync, encrypted DMs, media | a personal Messenger account syncs and sends; the sidecar is supervised and licence review passed |
| M4 remove Matrix | #133 | delete `alloy-core`, matrix-sdk/ruma deps, Matrix path of `alloy-push`, Matrix-only protocol variants and UI, Synapse fixtures/CI; Android foreground service | no Matrix dependency or UI remains; connectors stay alive on Android |
| M5 hardening | #134 | SQLCipher decision for the message DB, stable device fingerprints, per-connector rate limits, desktop + Android E2E with the fake connector | v0.4.0 released |
| M6 gateway (optional) | #135 | `alloy-gateway` hosting the same connectors on Postgres; clients sync by `event_log.seq` over WSS; encrypted Web Push to UnifiedPush | a client syncs from the gateway with no connector running locally |

Sequence: M0 → M1 → (M2 ∥ M3) → M4 → M5. M6 is post-milestone and must not shape M0–M5
contracts beyond keeping them serializable.

## References

- [Telegram API terms](https://core.telegram.org/api/terms)
- [Obtaining a Telegram api_id](https://core.telegram.org/api/obtaining_api_id)
- [mautrix/meta](https://github.com/mautrix/meta), [account-lock reports](https://github.com/mautrix/meta/issues/44)
- [grammers](https://codeberg.org/Lonami/grammers)
- [Synapse installation requirements](https://element-hq.github.io/synapse/latest/setup/installation.html)
- [Element Server Suite Classic requirements](https://docs.element.io/latest/element-server-suite-classic/requirements-and-recommendations)
- ADR 0006 — shared controls, tags and one members UI
- ADR 0008 — chat parity
- ADR 0009 — Android background push
- `docs/PLAN.md` §9 — Matrix removal roadmap
