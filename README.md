# Alloy

**A privacy-first, customizable Matrix chat client for desktop and Android.**

Rust core, Tauri 2 shell, Svelte 5 UI. End-to-end encrypted, zero telemetry, Apache-2.0.

> **Status: pre-release (v0.1.x).** Desktop (macOS, Windows, Linux) and Android work end to end against a
> real homeserver: sign in, encrypted chats, threads, polls, media, notifications, updates. Push
> notifications while the app is closed, OIDC login, search, plugins and the marketplace are not built yet.
> See the [roadmap](#roadmap).

## What works today

**Chat**
- Password sign-in with homeserver discovery, session restore, device verification (emoji SAS), recovery key.
- Encrypted and plain rooms on sliding sync (MSC4186, with MSC3575 fallback), history loads as you scroll.
- Send, edit, delete, reply, react, threads, polls, mentions, markdown, emoji, attachments with progress,
  image/video/audio viewer, read receipts, typing indicator, unread badges that clear when you read.
- Your own room tags (name, colour, order) to organise, filter and sort the room list; replies shown as readable quotes.
- Spaces, room directory, invites, new rooms and DMs, room info (name, topic, picture, private note,
  media/files/links/members), profile cards, right-click and long-press menus.

**Look and feel**
- Discord-style desktop layout with resizable panels; Modern, Bubbles or IRC message layouts;
  light, dark and AMOLED themes plus one extra; font size, time format, per-user name colours.
- A separate mobile UI under 900 px: one screen at a time, bottom tab bar, full-screen pages, bottom and
  action sheets, 44 px touch targets, Send button, a page that never scrolls.

**Settings that do something**
- Account (display name, avatar), devices, security and privacy, encryption keys, notifications
  (server push rules, desktop/Android notifications, per-room levels), preferences, keyboard reference, sidebar.

**Delivery**
- Installers for macOS (arm64, x64), Windows (exe, msi) and Linux (AppImage, deb, rpm) and a 36 MB Android APK, on the
  [releases page](https://github.com/centopw/alloy/releases). Desktop updates itself; Android tells you when
  a newer release exists. SBOM attached to releases. No telemetry.

## Roadmap

Full plan: [docs/PLAN.md](docs/PLAN.md). Board: [github.com/users/centopw/projects/2](https://github.com/users/centopw/projects/2).

| Train | Scope | State |
|---|---|---|
| P0 Foundations | workspace, spikes, CI, gate G0 | done |
| **v0.1 Desktop MVP** | the list above | **done** (v0.1.0 to v0.1.4) |
| **v0.2 to v0.4 Android and parity** | UnifiedPush/FCM push, OIDC login, mobile gestures, real-device perf (G1), a11y audit, encrypted search, QR login, voice messages, i18n and RTL, theme/keybinding JSON packages | in progress |
| v0.5 Plugins | sandboxed WASM plugins (Wasmtime host exists), 12 UI extension slots, capability prompts | planned |
| v1.0 Marketplace | signed packages and index, signed installers, Flathub/winget/Homebrew/F-Droid | planned |
| Post-1.0 | web, calls (MatrixRTC), multi-account, widgets, iOS | planned |

Not yet signed with paid developer certificates, so every release is marked pre-release.

## Installing a release

Builds are on the [releases page](https://github.com/centopw/alloy/releases):

- **macOS:** drag Alloy to Applications, then right-click it and choose Open (or allow it under
  System Settings > Privacy & Security > Open Anyway). From a terminal:
  `xattr -dr com.apple.quarantine /Applications/Alloy.app`. Use 0.1.2 or newer (0.1.1 had an unsealed bundle).
- **Windows:** `.exe` installer or `.msi`; SmartScreen may warn, choose More info, then Run anyway.
- **Linux:** AppImage, `.deb` or `.rpm`.
- **Android:** the APK is signed with a throwaway key, so uninstall the old version before installing a new one.
  Needs Android 7.0 (API 24) or newer; the Android 10+ low-end target is the one we tune for.

Once installed, desktop builds update themselves from Settings > Help & About.

## Development

### Prerequisites

- **Rust** 1.90+ (rustfmt, clippy) and **Node.js** 22+ with **pnpm** 9+ (`npm`, `yarn`, `bun` are not used)
- **just** (task runner), **cargo-deny**, **cargo-audit**
- **Docker** for the local Synapse used by integration and E2E tests
- Android builds: Android SDK, NDK 27 and `rustup target add aarch64-linux-android`

### Run it

```bash
git clone https://github.com/centopw/alloy.git && cd alloy
just hooks                 # commit-message hook (Conventional Commits)
just synapse-up            # local homeserver on :8008 (users alice and bob, password testpassword123!)
cd apps/alloy && pnpm install && (cd ui && pnpm install)
pnpm tauri dev             # desktop app
pnpm tauri android build --apk --debug --target aarch64   # Android debug APK
```

### Checks (all enforced in CI)

```bash
just ci                    # everything below
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
ALLOY_TEST_HS=http://localhost:8008 cargo test --workspace --no-fail-fast
cargo deny check && cargo audit
cd apps/alloy/ui && pnpm check && pnpm test && pnpm build
node scripts/e2e-bridge.mjs                              # UI + engine + Synapse, desktop
E2E_MOBILE=1 E2E_STRICT=1 node scripts/e2e-bridge.mjs    # phone audit: no page scroll, 44 px targets
```

Other recipes: `just --list`. Releasing: [docs/RELEASING.md](docs/RELEASING.md).

## Architecture

```
apps/alloy
  ui/            Svelte 5 + TypeScript (runes only), tokens -> CSS variables, virtua timeline
  src-tauri/     Tauri 2 shell: IPC glue, updater, notifications, Android project
        | typed commands + VectorDiff events (alloy-protocol, TS types generated by specta)
crates/
  alloy-protocol   DTOs and commands shared with the UI
  alloy-engine     command handlers, subscriptions, event sink
  alloy-core       matrix-sdk / matrix-sdk-ui wrapper: sync, timeline, crypto, media, TLS
  alloy-storage    SDK store + OS keychain (Android: file store)
  alloy-theme      design-token schema and CSS generator
  alloy-plugin     Wasmtime component host (limits, capabilities); not yet wired in
  alloy-bridge     dev WebSocket bridge used by the browser E2E
packages/          bundled themes, sample plugin     schemas/   JSON Schemas
```

Data flow is one way: UI -> IPC -> `alloy-engine` -> `matrix-sdk` -> `eyeball-im` diffs -> `VectorDiffDto` ->
Svelte reactive arrays. `#![forbid(unsafe_code)]` everywhere.

## Documentation

| | |
|---|---|
| [Master plan and roadmap](docs/PLAN.md) | decisions D1 to D14, phases, what changed, what is next |
| [Decision records](docs/decisions/README.md) | [0001 visual language](docs/decisions/0001-ui-visual-language.md), [0002 layout](docs/decisions/0002-discord-style-layout.md), [0003 UX review](docs/decisions/0003-ux-review-2026-10.md), [0004 functional settings](docs/decisions/0004-functional-settings.md), [0005 mobile shell](docs/decisions/0005-mobile-first-shell.md), [0006 shared controls, tags, one members UI](docs/decisions/0006-shared-controls-tags-and-one-members-ui.md) |
| [Detail docs](docs/plan/) | UI stack, core, extensibility, security and delivery, UX, IPC contract, theming |
| [Releasing](docs/RELEASING.md) · [CI](docs/CI.md) · [Tracking](docs/TRACKING.md) | pipelines, update feed, board workflow |
| [Changelog](CHANGELOG.md) | per release |

## Contributing, conduct, security

[CONTRIBUTING.md](CONTRIBUTING.md) (Conventional Commits, issue-first workflow) · [Code of Conduct](CODE_OF_CONDUCT.md) ·
[SECURITY.md](SECURITY.md): report vulnerabilities through GitHub Security Advisories or hiep@hce.vn with `[SECURITY]`.
Accepted dependency advisories, each with its reason, are listed in `deny.toml` and `.cargo/audit.toml`.

## License

Apache License 2.0, see [LICENSE](LICENSE).
