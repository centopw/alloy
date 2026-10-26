---
id: alloy-architecture
type: architecture-design
status: draft
title: Alloy — architecture
parent: alloy-goal
tags: [matrix, rust, tauri, svelte, architecture]
---

## Topology

Alloy is one Rust workspace plus a Tauri 2 application. The Rust side owns all product logic; the Svelte 5 view layer renders state and sends commands over a typed protocol. Three layers carry the system:

- **Contracts.** `alloy-protocol` holds the frozen Protocol v1 DTOs and tagged enums (`EngineCommand`, `EngineResponse`, `EngineEvent`, `EngineError`, `VectorDiffDto`). TypeScript types are generated from it into `apps/alloy/ui/src/lib/generated/`. User-authored packages are validated by the published JSON Schemas in `schemas/` (theme, keybindings, plugin manifest); WASM plugins are bound by `crates/alloy-plugin/wit/alloy-plugin.wit`.
- **Engine and core.** `alloy-core` is the matrix-sdk / matrix-sdk-ui facade: accounts, auth, rooms, timeline, crypto, media, notifications, search, room tags and URL previews. `alloy-engine` adds the `Engine` trait, `RouterEngine` dispatch to group handlers, the `EventSink` broadcast and the `SubscriptionRegistry`; it is deliberately decoupled from Tauri so handlers are unit-testable standalone. `alloy-storage` owns secret persistence; `alloy-theme` owns the design-token schema, loader and CSS-variable output.
- **Presentation and optional subsystems.** The Tauri shell (`apps/alloy`) exposes `alloy_invoke` and `alloy_subscribe`, selects the secret store, and provides notification, file-pick, save and update commands. The Svelte UI consumes generated protocol types only. `alloy-bridge` re-exposes the same engine over a loopback WebSocket for browser E2E; `alloy-push` and `alloy-ffi` carry Android background delivery; `alloy-plugin` hosts sandboxed WASM components.

A command travels UI → transport (Tauri IPC, mock transport in development, or the WS bridge) → `RouterEngine` → group handler → `AlloyClient` → homeserver. State changes return as `EngineEvent` streams; list changes use `VectorDiff` so the UI applies diffs instead of refetching.

## Contracts and boundaries

- **`alloy-protocol` is the base.** It must not depend on `alloy-core`, `alloy-engine` or `alloy-storage`. Everything else may depend on it. The protocol is frozen per phase and written by one owner before dependent streams start.
- **The UI never talks to the homeserver or matrix-sdk directly.** It sends protocol commands and applies protocol events. Transports are interchangeable: Tauri IPC in the app, a mock in development, the loopback bridge for browser E2E.
- **The engine stays Tauri-free.** Tauri-specific code lives only in `apps/alloy`; `alloy-engine` and below are framework-agnostic so they can be tested and driven headlessly.
- **Secrets have one owner.** Only `alloy-storage` persists credentials: the OS keyring where a backend exists, an owner-only file on Android and Linux without Secret Service. Crates pass a `SecretStore`; they never write secret material themselves.
- **`alloy-ffi` is the sole unsafe crate.** The workspace forbids `unsafe_code`; the JNI exports need the C ABI, so that crate carries a documented `unsafe_code = "allow"` override and catches panics at the boundary.
- **The plugin host is sandboxed.** Components run under fuel metering and a 32 MiB cap with capability denial; they emit declarative view schemas, never DOM or raw handles. The host currently has no edge into the engine.
- **Attachments are allowlisted.** `EngineContext` canonicalises paths and only lets `SendAttachment` read allowlisted files and roots.
- **Background push is headless and privacy-first.** `alloy-push` opens the same session data directory as the engine and fetches `event_id_only` payloads client-side; UnifiedPush is the default path. See ADR-0009 for the decision and its limitations.

## Dependency edges

The workspace DAG, as the manifests declare it:

- `alloy-protocol` — no internal dependencies.
- `alloy-storage` → protocol.
- `alloy-core` → protocol, storage.
- `alloy-engine` → protocol, core, storage, theme.
- `alloy-bridge` → engine, protocol, storage.
- `alloy-push` → core, storage.
- `alloy-ffi` → push.
- `alloy-app` (the Tauri shell) → protocol, engine, core, storage.
- `alloy-theme` and `alloy-plugin` — standalone; neither depends on another workspace crate.

The UI depends on the generated protocol types, not on a Rust crate. `alloy-plugin` has no inbound edge yet: the host exists and is tested, but the engine does not load plugins.

## Invariants

- `unsafe` is forbidden workspace-wide; only `alloy-ffi` overrides that lint.
- Protocol v1 is frozen: commands, responses and events change by a recorded decision and regenerated TS types, never ad hoc.
- Zero telemetry: no analytics, tracking, ads or remote logging. The Android default path avoids Google services.
- All product logic lives in Rust; the UI is a view layer.
- Every settings control changes real behaviour or does not exist (ADR-0004).
- Below 900 px the app is a phone shell, not a squeezed desktop (ADR-0005).
- One implementation per UI concept; audit tests fail raw `<select>`/`<textarea>`/text `<input>`, native context menus and tooltip/emoji drift.
- Design tokens are the single source of colour, type and spacing; the UI consumes generated CSS variables (ADR-0007).
- A command handler never bypasses `SecretStore` for credentials, and the engine never stores secrets outside `alloy-storage`.

## Known limitations

Current-state gaps owned by this graph; each module spec carries its own detail.

- **Search:** server-side search in encrypted rooms is unavailable (`alloy-core`); encrypted rooms are never sent to the homeserver index. Android content-URI attachment picking and pull-to-refresh are implemented in the shell.
- **Customization:** themes load from JSON/JSONC files in `<data_dir>/themes` and development theme directories; keybinding and slash-command JSON packages are not wired, the WASM plugin host is not wired into the engine, and no marketplace exists (`alloy-plugin`).
- **Quality and reach:** the accessibility audit is pending; G1 performance is measured on an emulator only; there is no iOS target; the web target is not built; installers are pre-release and unsigned. OIDC/MAS sign-in (#65) is not shipped. Push-specific limitations — FCM fallback deferred, single UnifiedPush registration, distributor app required — live in ADR-0009.
