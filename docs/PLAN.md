# Alloy — Master Plan

Cross-platform Matrix client. Rust core, modern chat UI, deep user customization (JSON packages + sandboxed WASM plugins + marketplace). Desktop and mobile first; web later.

Detail docs (research, evidence links, work-item IDs): [01-ui-stack](plan/01-ui-stack.md) · [02-core-architecture](plan/02-core-architecture.md) · [03-extensibility](plan/03-extensibility.md) · [04-security-delivery](plan/04-security-delivery.md) · [05-ux-design](plan/05-ux-design.md).
This file is authoritative where the detail docs disagree (see §2).

## 1. Decisions (locked)

| # | Decision | Source |
|---|---|---|
| D1 | **Shell: Tauri 2**. Targets for first versions: **macOS/Windows/Linux + Android only**; iOS deferred (D10). **UI: Svelte 5 + TypeScript + Vite**, virtualized timeline via `virtua`. All logic in Rust. *(user-confirmed)* | 01 |
| D2 | Fallback if WebView perf gate fails (low-end Android / Android Go, 60 fps scroll): **Flutter + flutter_rust_bridge** reusing `alloy-core`. Decided at Gate G1. | 01 |
| D3 | Matrix engine: `matrix-sdk` + `matrix-sdk-ui` (RoomListService, Timeline, send queue), MSC4186 sliding sync (MSC3575 proxy fallback), vodozemac E2EE, OIDC/MAS login with password fallback, authenticated media. | 02 |
| D4 | Core↔UI: typed command/event protocol (`alloy-protocol`), `VectorDiff` streams, TS types generated with specta/ts-rs. Transport = Tauri IPC now, WASM/worker later. | 01, 02 |
| D5 | Customization in 3 tiers: **A** declarative JSON(C) packages (themes, tokens, keybinds, slash commands, render rules); **B** WASM Component Model plugins (Wasmtime; Wasmi fallback); **C** signed static-index marketplace. Exactly 12 named UI extension slots; plugins emit declarative view schemas, never DOM/raw handles. | 03, 05 |
| D6 | **Calls (MatrixRTC) post-1.0.** *(user-confirmed)* | 02 |
| D7 | iOS WASM-plugin policy (declarative packages only, no code plugins) applies **whenever iOS is eventually added**; moot for now. *(user-confirmed)* | 03 |
| D10 | **No iOS in first versions** (user, 2026-10-16): ship desktop + Android only, distributed by direct install (signed installers/AppImage/deb/rpm/MSI, APK via GitHub Releases, F-Droid later) so no app-store publication is required before first version. iOS (and App Store/TestFlight, APNs, NSE, Pulley) moves to P5. Play Store optional later. Android push = UnifiedPush first (no Google dependency), FCM optional. | user |
| D8 | Apache-2.0. `forbid(unsafe_code)` workspace-wide; cargo-deny/audit in CI; zero telemetry by default. | 04 |
| D9 | Package format `.alloy-pkg`, signed Minisign/Ed25519; dev-mode sideload on desktop. Manifest files are **JSON/JSONC validated by published JSON Schema** (user asked for `.json`; TOML dropped). | 03 |

## 2. Cross-doc conflicts resolved

| Conflict | Resolution |
|---|---|
| 02 proposes UniFFI `alloy-ffi` Swift/Kotlin shells (Element X model); 01 chooses Tauri. | `alloy-ffi` kept **only** for background paths a WebView can't serve: Android UnifiedPush/FCM service (iOS NSE deferred with D10) (decrypt + notify without launching the UI). Not a UI shell. |
| 04 lists Tauri updater **and** Sparkle. | Tauri updater only (signed manifests). Sparkle dropped: needs native app-shell integration. Android APK sideload builds self-update via Tauri updater/in-app prompt; store builds (later) use the store's path. |
| 04 says SQLCipher; 02 says `matrix-sdk-sqlite`. | Use matrix-sdk-sqlite's own store encryption with key in OS keychain. SQLCipher only for app-level DBs if needed. **Verify in spike S4** `[INFERENCE]`. |
| 02 puts plugin host in Phase 3; 03 puts themes in v0.2; 05 puts customization tokens in MVP. | Tokens/themes ship with MVP, JSON packages Phase 2, WASM host Phase 3 (§4). |
| 01 UI-08 (APNs/FCM) vs 02 CORE-14/15. | Single work stream: CORE-14/15 own native side; UI-08 only wires events. |
| 01 notes Svelte; 05 says "CSS tokens". | Same thing: tokens → CSS custom properties. |
| Crate/library versions in 02 (`matrix-sdk-search`, "v0.19+") and 03 (`componentize-js`, Pulley on iOS) unverified by running code. | Gate G0 spikes S1–S6 confirm before dependent work. |

## 3. Architecture

```mermaid
flowchart TB
  subgraph UI["apps/alloy (Tauri 2 shell) — Svelte 5 + TS"]
    Shell[Adaptive shell / timeline / composer]
    Tok[Token engine → CSS vars]
    Slots[12 extension slots renderer]
  end
  UI <-->|alloy-protocol: commands + VectorDiff events (Tauri IPC; WASM later)| API
  subgraph Rust["Rust workspace"]
    API[alloy-app: IPC glue]
    Core[alloy-core: matrix-sdk-ui engine]
    Store[alloy-storage: sqlite store + keychain]
    Plug[alloy-plugin: manifest, caps, Wasmtime host, pkg verify]
    Theme[alloy-theme: token schema/loader]
    Proto[alloy-protocol: DTOs]
    FFI[alloy-ffi: Android push service only]
  end
  API --> Core & Plug & Theme
  Core --> Store
  Core --> Proto
  FFI --> Core
  Core -->|sliding sync, E2EE, media| HS[(Homeserver)]
  Plug -->|fetch index/pkgs| Mkt[(Static signed index + CDN)]
```

Repo layout:
```
Cargo.toml  justfile/xtask  deny.toml  rust-toolchain.toml
crates/alloy-{protocol,core,storage,theme,plugin,ffi}
apps/alloy/{src-tauri, ui}
schemas/      # JSON Schemas: theme, manifest, keybinds, ui-schema
packages/     # first-party themes/plugins, alloy-pdk-rust
tools/alloy-pack
docs/
```

## 4. Phases and gates

Gate = must pass before the next phase starts. Each phase lists parallel work units (one subagent each, disjoint paths). IDs reference detail-doc work breakdowns.

### P0 — Foundations & spikes (→ Gate G0/G1)
Serial prerequisite: **P0-A workspace bootstrap** (CORE-01, UI-01, security "workspace setup", LICENSE). Then in parallel:
- **S1** matrix-sdk-ui login + room list + timeline against local Synapse (CORE-02/03/05 skeleton).
- **S2** UI-02 type generation + UI-04 virtualized timeline prototype with 10k synthetic events.
- **S3** Tauri mobile: install Android SDK/NDK, build+run debug APK on emulator and a low-end profile; **perf gate G1** (scroll ≥ 55 fps, cold start budget) on real low-end Android hardware when available (emulator numbers are indicative only).
- **S4** Storage/keychain: matrix-sdk-sqlite encryption + `keyring`/Keystore.
- **S5** Wasmtime component host: Cranelift on desktop/Android (Pulley/iOS feasibility recorded, not needed now), 32 MB cap, fuel (EXT-04/05 prototype).
- **S6** CI skeleton: fmt, clippy, deny, audit, test matrix macOS/Win/Linux + mobile builds (04).
- **G0:** S1,S2,S4,S6 green. **G1:** S3 meets perf budget, else pivot per D2.

**P0 status (2026-10-16): G0 passed** — fmt/clippy `-D warnings`/tests green on the workspace, UI check+build green, `cargo deny` ok, integration tests pass against local Synapse 1.163 (native MSC4186, E2EE room decrypt). Spike reports: `crates/*/SPIKE.md`, `apps/alloy/SPIKE.md`, `docs/spikes/`. **G1 (perf) provisional**: Android emulator WebView p50 16.7 ms, p95 20–22 ms, p99 42–49 ms on 10k-event timeline (Pixel-class and 2 GB/720p profiles) — real low-end hardware still required for final G1. Verified: Wasmtime component host (fuel, 32 MiB cap, capability denial), `matrix-sdk(-ui)` 0.19.1 exists, matrix-sdk-sqlite StoreCipher sufficient (no SQLCipher).

### P1 — Desktop MVP (v0.1)
Parallel streams (after P0 contracts frozen: `alloy-protocol` v0, token schema v0):
- **Core:** CORE-03 auth (OIDC+password), 04 storage, 05 room list, 06 timeline, 07 E2EE/backup/SAS, 08 send queue, 09 authed media.
- **UI shell:** UX-03 adaptive shell, UX-04 timeline, UX-05 local echo, UX-06 composer, UX-07 command palette, UX-08 onboarding/.well-known/OIDC.
- **Design system:** UX-01/02 tokens + light/dark/AMOLED (+EXT-01 theme schema), UI-03 CSS engine.
- **Delivery:** code-signing CI, updater, SBOM, SECURITY.md/CONTRIBUTING.
- **Exit:** log in, E2EE chat, send/receive/edit/redact/reply/react, media, verify device, dark/light/AMOLED, on macOS+Windows+Linux; signed nightly builds; perf budgets in CI.

### P2 — Mobile + parity + Tier-A customization (v0.2–0.4)
- **Mobile:** UI-05 safe areas/gestures, CORE-14/15 `alloy-ffi` UnifiedPush (+ optional FCM) service, UI-08 event wiring; signed APK via GitHub Releases. iOS not in scope (D10).
- **Features:** CORE-10 reactions/polls, CORE-11 threads (UX-10), CORE-12 spaces, CORE-13 encrypted search, UX-09 QR login + SAS UX, UX-11 voice messages/polls UI, UX-12 Fluent i18n/RTL, a11y pass.
- **Customization Tier A:** EXT-01/02/03 (theme packages, hot reload, keybindings, slash commands), UX-14 settings screen, local `.json` package loading, JSON Schemas published.
- **Exit:** feature parity with Element X core flows; push works on both mobiles; users can load a custom theme/keybinding JSON.

### P3 — Tier-B plugins (v0.5)
Serial first: EXT-04 WIT + `alloy-protocol` plugin events (needs stable core API). Then parallel: EXT-05 engine, EXT-06 Wasmi fallback, EXT-07 capabilities/prompts, EXT-08 limits, EXT-09 scoped KV, EXT-10 `alloy-pdk-rust`, EXT-11 `.alloy-pkg`, UX-13 + UI-06/07 slots & declarative renderer & sandboxed iframe widgets, EXT-16 TS PDK (optional/nonblocking).
- **Exit:** sideloaded WASM plugin adds a command + panel + message renderer under capability prompts; quotas enforced; code plugins available on desktop + Android.

### P4 — Marketplace & 1.0
EXT-12 signing, EXT-13 index client, EXT-15 revocation, EXT-14 in-app extension manager, `alloy-community-index` repo + PR validation, CDN; distribution channels: GitHub Releases (installers + APK), Flathub, winget, Homebrew cask, F-Droid; Play Store optional; no App Store; security review + external audit of plugin host; device-farm E2E; a11y audit; docs site.
- **Exit (v1.0):** installable signed plugins/themes from marketplace; all budgets green; stores live.

### P5 — Post-1.0
Web target (WASM `alloy-core` + IndexedDB store, UI-09), MatrixRTC/LiveKit calls (CORE-16), multi-account, Matrix widgets (MSC1236), **iOS target** (Tauri iOS shell, APNs + Notification Service Extension, TestFlight/App Store, curated plugin catalog pending legal review), JS plugin runtime improvements.

## 5. Critical path
`P0-A → S1 (core skeleton) → protocol freeze → Core 05/06/07 ‖ UI shell/timeline → MVP → alloy-ffi/push → stable core API → WIT → plugin host → marketplace → 1.0`.
Highest risks: WebView perf on low-end Android (G1), Android background delivery without Google services (UnifiedPush), sliding-sync spec churn, Android OEM battery killers. (iOS risks deferred under D10.)

## 6. Orchestration rules for implementation
- Tracking: board <https://github.com/users/centopw/projects/2>; every unit of work is an issue; see [TRACKING.md](TRACKING.md) and [../AGENTS.md](../AGENTS.md). Commits carry `Refs #N`; gates are recorded as issue comments.
- One subagent per disjoint crate/dir; shared contracts (`alloy-protocol`, `schemas/`, token schema) are written first by a single owner and frozen per phase.
- Subagents edit only; orchestrator runs `cargo fmt`, `clippy -D warnings`, `cargo deny`, tests, `pnpm check`, and smoke runs once per phase.
- Phase commits only when gates green.
- Git: remote `origin` = https://github.com/centopw/alloy, branch `main`, author hiep@hce.vn. Until the first functioning version, push directly to `main` after every green logical unit (no red trees); keep GitHub current. History linear, one logical change per commit, Conventional Commits enforced by `scripts/check-commit-msg.sh` (hooks via `just hooks`, CI job `commits`). Stage explicit paths only; never `git add -A` while subagents are working. After the first functioning version, `main` becomes PR-only.

## 7. Open items (non-blocking, defaults chosen)
- Oldest device baseline: Android 10+ low-end devices (default); iOS out of scope.
- Default density: Normal; bubbles for DMs, gutter for channels (05 recommendation).
- Mobile tabs: 4 (Spaces, DMs, Mentions, Settings).
- Apple Developer ID / Play accounts: needed by P1 signing CI (owner: user).
- Telemetry: none in v1.
- Product name/branding/domain: "Alloy" placeholder.
