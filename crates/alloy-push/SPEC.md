---
id: alloy-push
type: module-design
status: draft
title: alloy-push — headless background notifications
parent: alloy-architecture
tags: [android, push, notifications]
depends-on: [alloy-core, alloy-storage]
---

## Responsibility

The headless background notification client for Android. It parses UnifiedPush and Matrix push payloads, fetches notification content with `matrix-sdk-ui`'s `NotificationClient` in multi-process mode (20-second timeout with a generic fallback), and manages HTTP pushers with the homeserver. It opens the same account data directory as the engine and uses `event_id_only` payloads so distributors never see message content.

## Boundary

It depends on core and storage; no Tauri and no UI. It runs one UnifiedPush registration and is consumed by `alloy-ffi`. It restores the account headlessly instead of duplicating the interactive engine session. Push-specific limitations — FCM fallback deferred, single registration, distributor app required — are recorded in ADR-0009.
