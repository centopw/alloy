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

The secret-persistence contract and its implementations. It defines the `SecretStore` trait and supplies three stores: `InMemorySecretStore` for tests, `FileSecretStore` (an owner-only file, the default on every platform and the only store on Android) and `KeyringSecretStore` for desktop OS keychains with per-OS backends, kept for callers that ask for it (`ALLOY_SECRET_BACKEND=keychain`) and for adopting the keys older builds sealed there. It also provides the owner-only file writer and the one-time keychain adoption. Access tokens and the store key live under the account's namespace; the account index itself carries no secrets.

## Boundary

It depends only on `alloy-protocol`. It must not depend on core or engine, and no other crate may persist credentials itself — callers receive a `SecretStore`. Keyring backends are chosen per OS; Linux deliberately omits the tokio feature because zbus's tokio `block_on` nests inside the application's runtime. The crate does no network or UI work.

Known limitations: Android keystore wrapping is not implemented (#74); the owner-only file is readable by any process running as the same user, which ADR 0019 accepts in exchange for never prompting for a keychain password.
