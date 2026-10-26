---
id: alloy-storage
type: module-design
status: draft
title: alloy-storage — secret persistence
parent: alloy-architecture
tags: [storage, secrets]
depends-on: [alloy-protocol]
---

## Responsibility

The secret-persistence contract and its implementations. It defines the `SecretStore` trait and supplies three stores: `InMemorySecretStore` for tests, `FileSecretStore` (an owner-only file used on Android and as the Linux fallback), and `KeyringSecretStore` for desktop OS keyrings with per-OS backends. It also provides the owner-only file writer. Access tokens and the Matrix store key live under the account's namespace; the account index itself carries no secrets.

## Boundary

It depends only on `alloy-protocol`. It must not depend on core or engine, and no other crate may persist credentials itself — callers receive a `SecretStore`. Keyring backends are chosen per OS; Linux deliberately omits the tokio feature because zbus's tokio `block_on` nests inside the application's runtime. The crate does no network or UI work.

Known limitations: Android keystore wrapping is not implemented (#74); a Linux system without a running Secret Service falls back to the owner-only file with a warning.
