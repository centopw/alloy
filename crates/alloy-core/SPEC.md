---
id: alloy-core
type: module-design
status: draft
title: alloy-core — Matrix domain engine
parent: alloy-architecture
tags: [matrix, engine]
depends-on: [alloy-protocol, alloy-storage]
---

## Responsibility

The Matrix domain engine: the facade over `matrix-sdk` and `matrix-sdk-ui`. It owns the account index and session restore, authentication (homeserver discovery, password login, logout; OIDC is tracked but not implemented), end-to-end encryption (recovery, cross-signing, SAS verification), rooms and timelines, authenticated media and room media listings, notification settings and push rules, server-side room search, user-defined room tags, account-synced preferences, URL previews and TLS root configuration. It maps Matrix SDK types to `alloy-protocol` DTOs and opens the encrypted SQLite store with its key from the secret store.

## Boundary

It depends on protocol and storage only — no Tauri, no UI types. It owns the interactive homeserver session; `alloy-push` runs a headless notification session in its own crate. The engine layer wraps this crate and never bypasses it. Search never sends encrypted rooms to the homeserver index.

Known limitations: server-side search in encrypted rooms reports `Unavailable`; OIDC/MAS login is not implemented (#65); Android's TLS platform verifier must be initialised through JNI before first use.
