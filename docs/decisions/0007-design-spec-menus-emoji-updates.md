# 0007: Workspace design spec, tiered menus, no native menus, emoji picker, update experience

- Status: accepted
- Date: 2026-10-19
- Extends: 0001, 0002, 0005, 0006

## Context
The maintainer supplied a workspace design spec (kept locally as `DESIGN.md`, not in the repository) and a reference layout: flat docked panes, hairline dividers, one blue accent, a list pane with filter chips and a single scrolling Details column. Reviews also asked for a better room menu, no OS-native right-click menus anywhere, a real emoji picker, and a proper update experience.

## Decisions
- **Design spec as tokens.** Dark and AMOLED themes take the spec's palette (canvas, secondary, elevated, active, hover, hairline, accent, status colours), 0px panel radius, 4px component radius, 6px inputs/composer/modals, rail 68px, list pane 240px (200-320), inspector 320px (320-420). Filled blue surfaces use a separate `accent.fill` (`#2563eb`) so white text keeps WCAG AA; `accent.primary` (`#3b82f6`) stays for lines, focus and indicators. New tokens: `surface.active`, `border.floating`, `scrim`, `accent.fill`.
- **Tiered menus with flyouts.** `MenuAction.submenu` opens a flyout beside the row (drill-in on phones) with optional search and a "create" row; `keepOpen` for toggles. The room menu is: Mark as read; Notifications; Tags (searchable, creates a tag from the typed name); Room settings; Copy; Leave. There is no "Open" entry.
- **No native menus.** A global guard suppresses the browser/WebView context menu; text fields get our Cut/Copy/Paste/Select all menu, links get Open/Copy link, and every interactive surface (tag chips, filter chips, space rail, header, composer buttons, timeline background, update pill) has its own menu via `use:contextmenu`. An audit test fails on ad-hoc `oncontextmenu` handlers.
- **One emoji picker.** `EmojiPicker` (data from `emojibase-data`, loaded lazily): category tabs, search, recently used, skin tones, keyboard navigation; popover on desktop, bottom sheet on phones. Composer and reactions both use it.
- **Details panel.** Room info is one scrolling column (identity, actions, About, Private note, Shared media, Files, Links, Members) with "View all" drill-ins; no tab strip.
- **Read state survives reload.** Read marks are persisted (`localStorage`), so a room read on this device stays read after a restart even when the client recomputes counts or the server's counters lag.
- **Updates.** Automatic check (8 s after start, then every 6 h) and automatic download are both user toggles; the release notes (the GitHub release body, copied into the update feed's `notes`) are shown natively in a changelog dialog, with "View on GitHub" still available. A quiet notice sits above the account card (all sizes) while an update is on offer: "Update available" with Download, then a progress hairline, then "Update ready" with Restart. It never blocks anything: the X (Remind me later) hides it for the session, the context menu adds Skip this version, and the app stays fully usable. There is no title strip. Tauri commands: `check_update`, `download_update` (progress events), `apply_update`. Android only reports and opens the release page.

## Consequences
New menus must declare submenus instead of long flat lists; new surfaces need a menu or are covered by the guard; filled accent UI uses `accent.fill`. Not done: call/video/presence UI (the engine has none), live cross-device tag sync, visual verification of the update pill against a real feed.

## Addendum (2026-10-19): search scopes, synced settings
- **Three searches, three purposes.** Sidebar and Ctrl/Cmd+K find rooms; the header, Details and Ctrl/Cmd+F search messages inside the open chat (server search for unencrypted rooms through `SearchRoomMessages`, local search of loaded messages for encrypted ones).
- **Settings split.** Preferences are either synced (global account data `chat.alloy.prefs`, newest `updated_at` wins, live updates through `AccountPrefsChanged`) or device-only (`DEVICE_PREF_KEYS` in `stores/prefs.svelte.ts`: layout, font size, notifications, Enter to send, autostart, update toggles).
- **No matrix.to link features.** Links built for matrix.to point away from the user's homeserver; copy actions use IDs instead.
