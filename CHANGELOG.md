# Changelog

## 0.2.4 (pre-release)

### Added
- **Permissions screen and first-run guidance**: A dedicated Settings > Permissions screen shows what Alloy asks the system for (notifications, microphone, location, push distributor, battery optimization), what each permission is used for, and its live status. A first-run prompt helps grant required permissions before using the app, and Settings > Notifications links directly to Permissions when notifications are blocked.
- **Reaction overlay in bubbles layout**: Reaction pills now sit directly on the bottom edge of message bubbles as an overlay instead of an inline block, matching modern messenger bubble design.

### Fixed
- **Desktop: clicking a message notification opens its chat.** Notifications now carry their room. Not yet verified on desktop or Android.
- **Android: the 0.2.2 and 0.2.3 APKs had version 1 (1.0) instead of 0.2.x.** Android does not install a lower version code over an installed app, so these two could not update 0.2.1 (version code 2001). The Android build now takes the app version from `Cargo.toml`, and the release workflow stops when the signed APK's version does not match the tag. Not yet tested on a device.

## 0.2.3 (pre-release)

### Added
- **Groundwork for F-Droid.** Store listing text, icon and screenshots (`fastlane/`), and a draft build recipe with the open points in `docs/FDROID.md`. A build made with `ALLOY_DISTRIBUTION=fdroid` leaves updates to the store and skips the release-page update check. Alloy is not on F-Droid yet.

### Fixed
- **Scrolling up through a chat no longer jumps around.** Loading older messages put the view back at the top of what had just loaded, and a freshly opened chat could sit at its oldest loaded message instead of the newest. Older messages from the same day are now added without moving the message you are reading.
- **The down-arrow count no longer includes messages you already read.** Loading history while scrolled up raised the "new messages" number; it now counts only messages that arrive after the newest one you have seen.
- **Security:** the plugin sandbox (wasmtime) is updated to 49.0.4, which fixes two advisories (RUSTSEC-2026-0378 and RUSTSEC-2026-0379).

## 0.2.2 (pre-release)

### Added
- **Refresh.** A Refresh button in the chat list header (Ctrl/Cmd+R or F5 on desktop, pull the chat list down on a phone, or "Refresh" in the timeline menu) restarts the connection to your server and reloads the open chat. Your open chat, reply and half-written edit stay as they were.

### Changed
- **Messenger-style long-press on phones**: pressing and holding a message dims the conversation with a spotlight keeping the pressed message bright, floats the emoji pill directly above the bubble, and shows a bottom action bar with large Reply, Copy (or Pin/Download), Forward, and More buttons.
- **Telegram-style desktop right-click**: the reaction strip floats as a separate pill directly above the rounded menu card, with message actions reordered to match Telegram (Reply, Edit, Copy text or picture, Pin/Unpin, Download, Forward, Select, Delete).
- **Copy picture**: copying an image, sticker, or GIF from the context menu or phone action bar converts it to PNG and places it on the system clipboard.

### Fixed
- **Latest messages sometimes missing until you restart.** Alloy's sync service stayed stopped after a network or server error. It now restarts itself, with a growing pause between attempts.
- **Reactions from the long-press pill and the right-click strip did nothing.** Tapping an emoji closed the menu without reacting.
- **Android long-press tap**: lifting your finger after pressing and holding a message no longer triggers the dimmed backdrop or a menu action.
- **Android emoji search**: the results no longer vanish when the keyboard opens.
- **Android picture paste**: pictures on the clipboard are read through the app, and the attach menu offers "Picture from clipboard" when there is one.
- Reaction chips are 44 px tall on phones.

## 0.2.1 (pre-release)

### Added
- **Notifications on Android while Alloy is closed**, without Google services (ADR 0009). Works with a UnifiedPush app such as ntfy. Turn it on in Settings > Notifications > "When Alloy is closed". New messages, including ones in encrypted rooms, show who wrote and what they said (or only "New message" when Show message preview is off); tapping one opens that chat. Nothing is shown while the app is open, and turning it off removes the registration from your server.
- **An offline notice.** When the connection drops, a bar says so and Alloy checks again every few seconds; settings and tags that failed to load come back by themselves when the connection returns.
- **Paste a picture into the message box** on desktop and Android: a screenshot copied as an image, or a picture file copied in your file manager or screenshot tool (which used to paste its path as text). The picture waits above the box with a preview; press Enter or Send picture to send it, or remove it.

### Fixed
- A lost connection no longer prints the raw request address and error in the sidebar; failures to reach the server read as one plain sentence.

## 0.2.0 (pre-release)

### Added
- **Messenger and Telegram style conversations** (ADR 0008):
  - **Reactions:** hover bar on desktop; on a phone press and hold a message for a pill of your emoji beside it, or double tap to react with your first emoji. Tap or right-click a reaction to see who reacted.
  - **Reply:** swipe a message to the right on a phone; the Up arrow in an empty message box edits your last message on desktop.
  - **Seen:** one check when sent, two when someone has read it, and avatars of readers under your newest message in small rooms (setting: Show who has read your messages).
  - **Pinned messages** with a bar under the header, **Forward** to several rooms, **Select** several messages to copy, forward or delete, and **Delete for everyone** with a confirmation (moderators can delete others' messages).
  - **Jump to latest** shows how many new messages arrived, and an `@` button jumps to messages that mention you.
- **Chat list:** pin chats to the top, archive them, mark as unread, mute for 1 hour, 8 hours, 24 hours or until you turn it back on, a "more" button on hover, swipe a chat left for Mute and Archive on a phone, and unsent text shows as "Draft:" in the list and comes back when you return.
- **Search all chats:** the sidebar search also finds message text across your unencrypted rooms.
- **Media:** pictures sent together appear as one album; the viewer is a gallery (swipe or arrow keys, pinch or double tap to zoom, swipe down to close, sender and time); audio plays inline with a waveform and 1x / 1.5x / 2x speed; Android can now download media to the Downloads folder; the attach button takes several files at once.
- **Voice messages:** hold the microphone on a phone (slide left to cancel, up to lock) or click it on desktop. Voice notes from other Matrix clients play too.
- **Writing:** a formatting toolbar on selected text (and an "Aa" row on phones) with Bold, Italic, Strikethrough, Code, Spoiler, Quote and Link, plus Ctrl/Cmd shortcuts; `||spoilers||` stay blurred until clicked; `@room` pings everyone when you are allowed to.
- **Location:** send your location or share it live for 15 minutes, 1 hour or 8 hours, with a banner and Stop button; received locations show a card that opens a map in your browser. No map is loaded inside the app.
- **Safety:** sign out one or all other sessions, change a member's role, remove or ban members, report a message, and block people (Settings > Security and privacy lists and unblocks them).

### Changed
- The update prompt is a small, quiet notice above your account card instead of a bright button in an extra top bar. It never interrupts: dismiss it with the X (it returns next launch), skip a version from its menu, or keep chatting and restart whenever you like.
- Opening a picture or video fills the whole window on every platform, with the picture on the plain background.
- Pictures, videos and stickers sent without a caption sit straight on the timeline: no bubble or panel behind them, and a bare file name is no longer shown as a caption.
- Phones: tapping the message box no longer jumps the conversation to older messages when the keyboard opens.
- **Quick reactions** are yours to choose in Settings > Preferences > Reactions (synced across your devices).
- Start typing anywhere in a conversation and the text goes into the message box, no click needed.
- Read receipts are now tracked in every conversation (needed for the seen marks).
- Deleting a message asks first.

## 0.1.9 (pre-release)

### Added
- **Link previews:** messages with a link show a card (site, title, description, picture) built by your homeserver. Setting in Preferences: Off, Unencrypted rooms only (default), or All rooms. Right-click a card to open or copy the link, or hide it.
- **Download media:** a Download button in the image/video viewer and a Download entry in the message menu save the file through the system Save dialog (desktop). Android hides the action until saving to the public Downloads folder is verified.
- README rewritten for people, with a banner, screenshots, live project stats and an activity section; `scripts/readme-shots.mjs` regenerates the screenshots from the built-in demo data.

### Fixed
- The full-size image/video viewer fits the window instead of overflowing.
- Android: the web layer no longer adds the status-bar inset on top of the native one (an empty band at the top on newer WebViews); the update button sits in the Chats header on phones instead of a separate strip.
- The demo mode no longer errors when loading older messages and shows a believable conversation.

### Added
- **Search inside a chat:** the header Search button, the Details Search button and Ctrl/Cmd+F open a message search for the current room: by text and by sender (`from:name` or the From menu), filtered by All / Media / Links / Files, with highlighted matches; clicking a result jumps to the message, loading older history if needed. Encrypted rooms (which the server cannot search) are searched on this device, with a button to search older messages. The sidebar search and Ctrl/Cmd+K remain the room switcher ("Search rooms").
- **Settings sync across devices:** preferences that describe how you like the app (time format, room list options, sort, typing and event toggles, autoplay, ...) are stored in your Matrix account data and follow you to other devices; changes from another device apply live. Settings that depend on the device (message layout, font size, notifications, Enter to send, open at login, update toggles) stay on the device.
- Details panel: a bell button for the room's notification level (full labels, current level checked); **Copy ID** copies the other person's Matrix ID in a DM (the room ID otherwise); one **Shared** section with Media / Links / Files tabs.
- The room picture changes only by pressing the picture (header or Details): Replace or Remove.

### Changed
- All matrix.to "Copy link" actions (rooms, spaces, messages) are removed.
- Buttons and inputs share one control height (36px, 44px on phones), so a field and its button line up.

### Fixed
- Choosing a tag for a room from its menu now shows at once (check mark, dots, chip counts) and rolls back on failure.

## 0.1.8 (pre-release)

### Added
- **Redesign (ADR 0007):** the workspace follows the new design spec: flat docked panes with hairline borders, 68px rail, 240px list pane with All / Direct / Channels / Unread chips, a single scrolling Details panel (About, Private note, Shared media, Files, Links, Members with "View all"), 6px-radius inputs and composer, one blue accent.
- **Tiered room menu:** Mark as read, Notifications, Tags, Room settings, Copy, Leave. Hovering Notifications, Tags, Room settings or Copy opens a small menu beside it. The Tags menu is scrollable, has a search field, and typing a new name offers "Create tag" (Enter). There is no "Open" entry any more.
- **No native right-click menus anywhere.** Every surface has our own quick menu: tag chips (rename, colour, move, delete), filter chips, sort button, space rail, chat header, composer buttons, timeline background, and text fields (Cut/Copy/Paste/Select all), links and selections.
- **Emoji picker:** category tabs, search, recently used, skin tones, keyboard navigation; used by the composer and by reactions.
- **Updates:** automatic check every 6 hours (toggle), automatic download (toggle, desktop), native changelog dialog with the release notes ("View on GitHub" stays available), an "Update" button in a slim title strip that becomes "Restart to apply update" after the download.

### Fixed
- A room you read stays read after a restart (read marks are stored on the device).
- Manage tags opens Settings directly on the Tags page.
- Buttons and filter chips are full-size touch targets on phones.

## 0.1.7 (pre-release)

### Added
- **Custom room tags** (ADR 0006): define your own tags (name, colour, order) in Settings > Tags, toggle them from a room's menu, filter the room list by a tag and sort it by activity, name, unread or tag. Tags are standard Matrix room tags (`u.<id>`), so other clients see them; the catalogue syncs through account data. New commands `GetTagCatalog`, `SaveTagCatalog`, `SetRoomTag`, `DeleteRoomTag`.
- Replies show a proper quote (sender name, two-line excerpt) above the text in every layout; clicking it jumps to the original message when it is loaded.
- GitHub release pages now list highlights, the commits since the previous version grouped by type, and a compare link (`scripts/release-notes.sh`).

### Changed
- One members UI: the separate members sidebar is gone; the header people button opens Room info > Members.
- Dropdowns and text boxes use our own controls instead of the browser's (`Select`, `TextField`, `TextArea`); enforced by a test.
- Settings are platform-aware: phones no longer show the Keyboard page or Enter-to-send, Android no longer shows "open at login", the Sidebar page becomes "Chat list" on phones.

### Fixed
- Hovering a message in the bubbles layout no longer makes it grow and push the messages below it.
- The unread badge no longer stays on after you have read a room (a stale server counter is ignored once you have read the room, until something newer arrives).
- Text areas no longer show the browser's resize grip.

## 0.1.6 (pre-release)

### Fixed
- Help & About: the Updates row no longer scatters its status text and buttons across the card. Status and actions share one column aligned to the same right edge as the other values, with the label on the first line; the "Check for updates" button is outlined, and the source link is a 44 px target on phones.
- Buttons use the app font instead of the browser's default button font.
- The phone E2E audit now covers Help & About.

## 0.1.5 (pre-release)

### Documentation
- README rewritten for the current state (what works, roadmap, install, development, architecture, doc index); `apps/alloy/README.md` replaces the template text.
- `docs/PLAN.md`: decisions D11 to D14 (layout, functional settings, mobile shell, advisory and release policy), P1 marked complete with the shipped v0.1.x, P2 status, and a roadmap section (what changed against the plan, now, next, later).

### Security / CI
- GitHub Actions moved off the deprecated Node 20 runtime: `actions/checkout` v7, `setup-node` v7, `setup-java` v6, `pnpm/action-setup` v6, `upload-artifact` v7.
- The daily audit runs `cargo audit` and `cargo deny` directly instead of `rustsec/audit-check`, which opened one GitHub issue per advisory (including ones deliberately accepted in `deny.toml` / `.cargo/audit.toml`). A new or unaccepted advisory now fails the job instead.

### Fixed
- Rooms now load their older history by themselves when the newest messages do not fill the screen (and when the window grows), instead of showing only the latest few messages until you could scroll.
- The unread badge clears once a room is read: the newest message counts as read as soon as it is on screen (opening the room, a message arriving while you are at the bottom, returning to the window), and the room list uses the client-computed unread counts, which drop the moment the read receipt is sent, instead of homeserver counters that can stay stale.

## 0.1.4 (pre-release)

### Changed
- Phones get a real mobile UI (ADR 0005): bottom tab bar, one screen at a time, full-screen room info/members/thread/settings pages, bottom sheets for dialogs and menus, long-press message actions, a Send button, 44px touch targets, bubbles by default.

### Fixed
- The page no longer scrolls or bounces on phones; only lists scroll.
- Long room names no longer run under the header buttons; the composer placeholder stays on one line.

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
