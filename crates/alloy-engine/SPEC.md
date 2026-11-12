---
id: alloy-engine
type: module-design
status: draft
title: alloy-engine — command router, account actors and projections
parent: alloy-architecture
tags: [ipc, engine, connectors, store]
depends-on: [alloy-protocol, alloy-store, alloy-connector, alloy-storage, alloy-theme, alloy-telegram, alloy-sidecar]
---

## Responsibility

The framework-agnostic command layer between the Tauri app and the on-device connectors (ADR 0016).
It owns the `Engine` trait, `RouterEngine` dispatch by command group (accounts, rooms, timeline,
media, settings), the group handlers, the `EventSink` broadcast, the `SubscriptionRegistry`, the
`EngineContext` (store, running accounts, pending logins, connector factory, data directory, secret
store, subscriptions, background flag, attachment allowlist) and the background machinery:

- one event actor per connected account, applying `alloy_connector::ConnectorEvent`s to
  `alloy-store` (chats, messages with `txn_id` reconciliation, edits, deletions, reactions, read
  markers, typing, rotated sessions) and to the connection status events;
- one outbox worker per account, performing queued send/edit/delete/react/read/typing operations
  through the same connector with exponential retry backoff, parked permanent failures and
  `NeedsRelogin` handling on `Unauthorized`;
- the startup sync: chat list pages plus the newest history of the most recent chats that have no
  local messages;
- the projections: the room list and one timeline window per open chat, emitted as
  `VectorDiffDto` values computed from the store and refreshed through a `ChangeBus` broadcast
  (no polling), including backwards pagination with connector backfill.

Device-local files that do not belong in the database live under the data directory: the tag
catalogue (`tags.json`) and drafts (`drafts.json`), written atomically.

## Boundary

It depends on protocol, store, connector, storage, theme, telegram and sidecar; it does not depend
on `alloy-core` or the Matrix SDK. Tauri types do not appear here; secrets reach the store only
through the injected `SecretStore`, and connector sessions are opaque bytes sealed by the store.
Commands arrive as protocol types and errors map to `EngineError` with stable codes; events are
broadcast to subscribers. Attachment paths are canonicalised and allowlisted in this layer, and a
command that needs a capability the account lacks fails with `not_supported` before anything is
queued.

Known limitations: the plugin host is not wired in; typing indicators are projected from connector
events only; `ChatRemoved` archives the chat because the store has no hard delete.
