# Changelog

## 0.1.3 (pre-release)

### Added
- Settings that configure behaviour (ADR 0004): Account (display name, avatar), Devices, Security & Privacy (typing notifications, clear media cache), Encryption (session keys, recovery), Appearance (message layout Modern/Bubbles/IRC, font size, time format, big emoji, theme), Notifications (server push rules, desktop notifications, per-room levels), Preferences (Enter to send, typing, join/leave and profile events, autoplay, open at login), Keyboard reference, Sidebar (previews, sections, People, Home scope).
- Desktop and Android notifications when a room that notifies gets new messages.
- Typing indicator, readable state events ("alice joined", "bob renamed the room").
- Room notification level in the room menu (default, all, mentions, mute).
- Room info panel (click the room title): name, topic and picture editing when permitted, a private per-room note, and Media, Files, Links and Members tabs (#28).
- Profile card for any user: from names, avatars, members and `matrix.to` mention links, with Message and Copy ID.
- Right-click menus on rooms (open, mark read, details, rename, picture, note, copy link/ID, leave), messages and members; long-press and the Menu key also open them.
- Resizable sidebar and side panels (drag, arrow keys, double-click to reset); widths are remembered.
- Tooltips for every labelled control, shown on hover and keyboard focus, enforced by a static audit test.
- Engine commands: `SetRoomName/Topic/Avatar`, `GetRoomNote/SetRoomNote`, `MarkRoomRead`, `GetUserProfile`, `StartDirectMessage`, `GetRoomMedia`.

### Changed
- Settings pages are reorganised after Element's: User settings, App settings, Info.

### Fixed
- Android: entering a server and pressing Continue no longer closes the app. The HTTP stack's platform certificate verifier aborted the process on Android; Alloy now trusts the system certificate store directly (also used by the update check).
- Senders no longer show as raw Matrix IDs: room members are fetched for every timeline and names fall back to the ID's local part.
- One layout rhythm (16px gutters, 48px headers) across chat, sidebar, panels, composer and settings; settings title rows line up.
- Public room directory rows keep the Join button aligned when a room name is long; the rail's Home entry is an icon; the search hint shows Cmd K on macOS.

## 0.1.2 (pre-release)

### Fixed
- macOS: the app bundle is now ad-hoc signed as a whole. 0.1.1 only had the linker's signature on the binary, so Gatekeeper reported "Alloy.app is damaged" on downloaded builds. It now shows the normal unidentified-developer prompt; notarization still needs an Apple Developer ID (#51).

## 0.1.1 (pre-release)

First build with in-app updates: installs of 0.1.0 have no updater and need a manual install of this version.

### Added
- In-app updates: desktop apps download, verify (minisign) and install new releases from Settings > About or the startup banner; Android tells you a newer release exists and opens its page (#51).
- Threads: thread panel with a thread-scoped composer, summary chips on thread roots, `SubscribeThread` and `in_thread_root` in the engine protocol (#27).
- Public room directory: browse and join rooms through `SearchPublicRooms` (#28).

- Polls (MSC3381): create, vote, end and live results in the timeline (#30).

### Security
- Dependency advisories: `cargo update` cleared the yanked `inout`; `deny.toml` and `.cargo/audit.toml` now check unmaintained and unsound crates too, with every accepted exception listed with its reason (transitive crates from tauri, matrix-sdk, keyring and tauri-plugin-updater), so any new advisory fails CI.
Findings from the threat-model review (#37):
- Desktop secrets were never persisted: the `keyring` crate had no backend enabled and used an in-memory mock. macOS, Windows and Linux backends are now enabled and the store fails closed; Linux falls back to an owner-only file when no Secret Service is running.
- A transient keychain error no longer overwrites the store passphrase; secrets are removed on logout, failed login and account replacement; the secrets file is created 0600 from the start.
- `SendAttachment` only accepts files the user picked (path allowlist); the unused local `GetMedia` source is gone.
- The dev bridge requires a token and a loopback Origin.
- Plain-http homeservers are refused except loopback and the Android emulator host.
- Decrypted media cache is cleared on logout and created owner-only.
- CSP gains `base-uri`/`form-action`/`object-src`; opener permissions are minimal; Android backups are disabled; release builds no longer read themes from the working directory; the HTML sanitizer drops `data-*`/`aria-*` and has tests.

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
