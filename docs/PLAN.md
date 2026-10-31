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
| D11 | **Discord-style layout** on desktop: space rail, room list, chat, optional side panels; gutter/bubbles/IRC message layouts are a user setting. | [0002](decisions/0002-discord-style-layout.md) |
| D12 | **Settings are functional or absent.** Every control changes real behaviour, either an account setting through an engine command or a device preference in the `prefs` store; nothing decorative. | [0004](decisions/0004-functional-settings.md) |
| D13 | **Mobile is its own shell, not a squeezed desktop** (width <= 899px): one screen at a time, bottom tab bar (Chats, Discover, Settings), full-screen pages, bottom/action sheets, 44px touch targets, a page that never scrolls. Guarded by a mobile E2E audit in CI. | [0005](decisions/0005-mobile-first-shell.md) |
| D14 | **Advisories are explicit.** Unmaintained or unsound transitive dependencies with no upstream fix are accepted one by one, with a reason, in `deny.toml` and `.cargo/audit.toml`; the daily audit fails on anything new. Releases are pre-releases until signing certificates exist; desktop updates through a signed feed (`updater-feed` release), Android only reports a newer release. | [RELEASING](RELEASING.md) |
| D15 | **DRY UI and custom room tags.** One implementation per UI concept (shared `Select`/`TextField`/`TextArea`, members only in Room info, settings filtered per platform); users define their own room tags stored as Matrix `u.<id>` tags plus a `chat.alloy.room_tags` catalogue. | [0006](decisions/0006-shared-controls-tags-and-one-members-ui.md) |
| D16 | **Design spec as tokens, menus as a system.** Dark/AMOLED follow the workspace design spec (`accent.fill` for filled blues keeps AA); tiered context menus with flyouts, search and create rows; no OS-native context menu anywhere; one emoji picker; updates are checked on a schedule, shown with a native changelog and applied with a restart button. | [0007](decisions/0007-design-spec-menus-emoji-updates.md) |

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

**P1 status (2026-10-19): complete. Shipped as pre-releases v0.1.0 to v0.1.4 (macOS arm64/x64, Windows, Linux, signed-with-a-throwaway-key Android APK).** Engine: auth (discovery, password, restore, logout), room list, timeline with local echo, edit/redact/reply/react, receipts/typing, recovery + SAS verification, authenticated/encrypted media, room admin (name/topic/avatar/note), user profiles and DMs, notification rules, devices and profile editing; all covered by Synapse integration tests (`crates/alloy-engine/tests`). UI: onboarding, Discord-style shell, three message layouts, composer with markdown/mentions/attachments/polls, threads, room info and profile cards, context menus, resizable panels, command palette, ten functional settings pages, desktop notifications, in-app updates, per decisions 0001 to 0005. Full-stack proof: `apps/alloy/ui/scripts/e2e-bridge.mjs` (desktop and `E2E_MOBILE=1` audits against `alloy-bridge` + Synapse, both in CI). Still open from the P1 exit: OIDC login (#65), perf budgets in CI (#69, #70), real-device perf (G1, #26), a11y audit (#20, #36), and signing certificates for notarized/Authenticode/Play-style distribution (#51, owner: user).

### P2 — Mobile + parity + Tier-A customization (v0.2–0.4)
- **Mobile:** UI-05 safe areas/gestures, CORE-14/15 `alloy-ffi` UnifiedPush (+ optional FCM) service, UI-08 event wiring; signed APK via GitHub Releases. iOS not in scope (D10).
- **Features:** CORE-10 reactions/polls, CORE-11 threads (UX-10), CORE-12 spaces, CORE-13 encrypted search, UX-09 QR login + SAS UX, UX-11 voice messages/polls UI, UX-12 Fluent i18n/RTL, a11y pass.
- **Customization Tier A:** EXT-01/02/03 (theme packages, hot reload, keybindings, slash commands), UX-14 settings screen, local `.json` package loading, JSON Schemas published.
- **Exit (approved 2026-10-31):** every row below is checked or explicitly deferred by the maintainer. D10 applies: "push works on both mobiles" means Android plus desktop notifications (desktop shipped); there is no iOS target.

| P2 exit item | Issue | State (2026-10-31) |
|---|---|---|
| Reactions, polls, threads, spaces, room directory | #30, #27, #28 | shipped |
| Mobile shell: back stack, safe areas, keyboard | #25 | shipped; swipe, pull-to-refresh and haptics open |
| `content://` attachments | #72 | shipped |
| Desktop notifications | — | shipped |
| Voice messages | #33 | shipped |
| Android push proven end-to-end (emulator + CI; real device pending) | #24 | in progress (wave 1) |
| QR login | #32 | not started |
| Encrypted local search | #31 | not started |
| Fluent i18n + RTL | #34 | in progress (wave 1) |
| Tier-A JSON packages | #35 | in progress (wave 1) |
| a11y audit | #36 | in progress (wave 1) |
| Keystore wrapping + secrecy types | #74 | not started |
| Perf baseline measured, then the budget enforced | #69, #70 | #69 in progress (wave 1); #70 open |
| Time-to-first-room-list instrumented | #70 | open (no instrumentation yet) |

**P2 status (2026-10-19): about half done.** Done: reactions and polls (#30), threads (#27), spaces and room directory (#28), mobile-first shell with back-button flow, safe areas and keyboard handling (ADR 0005, #25 partly), Android release APK of 36 MB with an update check (#73), Android sign-in fixed (system CA store), functional settings and notifications (ADR 0004). Not started: push delivery via UnifiedPush/FCM and `alloy-ffi` (#24, the biggest gap for real mobile use), content:// attachments (#72), encrypted search (#31), QR login (#32), voice messages (#33), Fluent i18n/RTL (#34), Tier-A package loading (#35: four themes ship, but compiled in, not loaded from user JSON), a11y audit (#36), Android keystore wrapping (#74). **P2 status (2026-10-31):** that paragraph is partly stale; the exit checklist below is authoritative, and wave 1 of the closeout covers #24, #35, #34, #69 and #36.

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
- Mobile tabs: 3 (Chats, Discover, Settings), decided in ADR 0005 after the 4-tab proposal.
- Apple Developer ID / Play accounts: needed by P1 signing CI (owner: user).
- Telemetry: none in v1.
- Product name/branding/domain: "Alloy" placeholder.

## 8. Roadmap (2026-10-19)

### What changed against the plan above
- **Crates:** an `alloy-engine` crate (command handlers, subscriptions, event sink) sits between the Tauri app and `alloy-core`, and `alloy-bridge` exposes it over WebSocket for the browser E2E. `alloy-ffi` and `alloy-push` now exist for Android background push; `tools/alloy-pack` does not exist yet (P3 packaging).
- **Customization:** the token system, light/dark/AMOLED and one extra theme ship, and `schemas/` hold the theme, keybinding and plugin-manifest schemas, but themes are compiled in; `alloy-plugin` has the Wasmtime host (fuel, 32 MiB cap, capability denial) and a sample `hello-plugin`, not yet wired into the engine.
- **Layout and mobile:** D11 and D13 replaced the single adaptive layout of the first plan.
- **Delivery:** GitHub Releases with a nightly channel, signed update feed for desktop, ad-hoc signed macOS bundle, SBOM; all releases are pre-releases.
- **Performance:** G1 stays provisional (emulator only: p95 20-22 ms frames); release APK 36 MB.

### Now (v0.2 train: Android and parity)
| Item | Issue | Why now |
|---|---|---|
| Push notifications without Google services (UnifiedPush, optional FCM, `alloy-ffi`) | #24 | Phones get nothing while the app is closed |
| OIDC / MAS login | #65 | matrix.org and other large servers are moving to MAS |
| Pull to refresh, `content://` attachments (swipe to reply shipped in 0.2.0) | #25, #72 | Finish the mobile shell |
| Real-device G1 and idle-memory budgets | #26, #69, #70 | Confirms or triggers the D2 fallback |
| Keyboard/screen-reader baseline, then full a11y audit | #20, #36 | Needed before 1.0; cheap now |
| Android keystore wrapping, secret types for passwords | #74 | Security debt from #37 |

### Next (v0.2 to v0.4)
Encrypted search (#31), QR login and verification polish (#32), Fluent i18n and RTL (#34), Tier-A packages: load theme/keybinding/slash-command JSON from disk with hot reload (#35). Voice messages (#33) and the Messenger/Telegram chat parity set (ADR 0008) shipped in 0.2.0.

### Later
v0.5 plugins (#38 to #46), v1.0 marketplace, signing and distribution (#47 to #55; signing certificates and store accounts are the user's), Post-1.0 (#56 to #60).

### Standing rules for new work
Every screen passes the mobile audit; every setting changes behaviour; every dependency advisory is accepted explicitly or fixed; every bug fix lands with a test that fails without it.
