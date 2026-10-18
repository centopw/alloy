# Changelog

## Unreleased

### Added
- Threads: thread panel with a thread-scoped composer, summary chips on thread roots, `SubscribeThread` and `in_thread_root` in the engine protocol (#27).
- Public room directory: browse and join rooms through `SearchPublicRooms` (#28).

### Fixed
- Android: attachments picked from content:// URIs are copied to the app cache before upload (#72); the device behaviour is unverified.
- Android: sessions are kept across restarts (secrets file in app-private storage; the keyring has no Android backend).
- Android: content respects status/gesture bars and the soft keyboard; Back closes dialogs, settings sections and chats before leaving the app.
- Phones: settings is a section list that drills into one section; the members panel starts closed on narrow screens and can be closed.

### Changed
- Android nightly and release builds ship a stripped, signed release APK (72 MB) instead of a debug APK (453 MB).

## 0.1.0 (pre-release)

First functioning desktop build. Installers are unsigned; the Android APK is debug-signed arm64 until a release keystore is configured.

### Added
- Rust engine (`alloy-engine`) over matrix-sdk 0.19 with sliding sync: server discovery, password login, session restore, multi-account index, logout.
- Rooms and timeline: room list with filters and search, invites, create/join, local echo, reply, edit, delete, reactions, read receipts, typing, back-pagination.
- End-to-end encryption: encrypted rooms, recovery key, cross-signing, emoji (SAS) device verification, encrypted media.
- Authenticated media download with cache, attachments, thumbnails.
- Svelte 5 UI in a Discord-style layout (ADR 0002): space rail, room sidebar with a user panel, flat message rows with avatars and per-user name colours, members panel, rounded composer with markdown, emoji, `@mention` autocomplete and upload progress, media lightbox, command palette (Ctrl+K). Theme, security and advanced options live in a settings menu.
- Room members, room details, attachment upload progress and space membership commands in the engine protocol.
- JSON theme packages compiled to CSS tokens (`gen-css`).
- Loopback WebSocket bridge (`alloy-bridge`) and full-stack E2E against Synapse.
- Tauri 2 desktop shell (macOS, Windows, Linux) and Android arm64 build.

### Known gaps
- OIDC login (#65).
- Release-build budgets (#69): idle memory 205 MB vs the 150 MB budget and a slow first cold start (3.4 s p95 vs 2.0 s) were measured on macOS arm64; time-to-first-room-list is not measured yet.
- Accessibility audit (#36) and real low-end Android measurements (#26) pending.
- No code signing or notarization; the updater signature key is not configured.
