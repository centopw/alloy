# Changelog

## 0.1.0 (pre-release)

First functioning desktop build. Installers are unsigned; the Android APK is debug-signed arm64 until a release keystore is configured.

### Added
- Rust engine (`alloy-engine`) over matrix-sdk 0.19 with sliding sync: server discovery, password login, session restore, multi-account index, logout.
- Rooms and timeline: room list with filters and search, invites, create/join, local echo, reply, edit, delete, reactions, read receipts, typing, back-pagination.
- End-to-end encryption: encrypted rooms, recovery key, cross-signing, emoji (SAS) device verification, encrypted media.
- Authenticated media download with cache, attachments, thumbnails.
- Svelte 5 UI: Telegram-style bubbles with grouping and per-user name colours, light/dark toggle, command palette (Ctrl+K), composer with markdown and emoji, settings and verification dialogs.
- JSON theme packages compiled to CSS tokens (`gen-css`).
- Loopback WebSocket bridge (`alloy-bridge`) and full-stack E2E against Synapse.
- Tauri 2 desktop shell (macOS, Windows, Linux) and Android arm64 build.

### Known gaps
- OIDC login (#65), `@mention` autocomplete (#66), room details drawer (#67), upload progress (#68).
- Cold start, idle memory and time-to-first-room-list not yet measured on release builds (#69).
- Accessibility audit (#36) and real low-end Android measurements (#26) pending.
- No code signing or notarization; the updater signature key is not configured.
