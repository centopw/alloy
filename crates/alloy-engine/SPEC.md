---
id: alloy-engine
type: module-design
status: draft
title: alloy-engine — command router and event fan-out
parent: alloy-architecture
tags: [ipc, engine]
depends-on: [alloy-protocol, alloy-core, alloy-storage, alloy-theme]
---

## Responsibility

The framework-agnostic command layer between the Tauri app and `alloy-core`. It owns the `Engine` trait, `RouterEngine` dispatch by command group (auth, rooms, timeline, crypto, media, spaces, settings), the group handlers, the `EventSink` broadcast, the `SubscriptionRegistry` and the shared `EngineContext` (client handle, data directory, secret store, subscriptions, background flag, attachment allowlist). It is decoupled from Tauri so handlers can be unit-tested standalone.

## Boundary

It depends on protocol, core, storage and theme. Tauri types do not appear here; product state persists through core and storage, and secrets only through the injected `SecretStore`. Commands arrive as protocol types and errors map to `EngineError`; events are broadcast to subscribers. Attachment paths are canonicalised and allowlisted in this layer. It does not load plugins and does not run background push.

Known limitations: the plugin host is not wired in; OIDC commands exist in the protocol but core returns them as not implemented.
