# 0006: Shared controls, one members UI, platform-aware settings, custom room tags

- Status: accepted
- Date: 2026-10-19
- Extends: 0002 (the dedicated members column is superseded), 0004, 0005

## Context
Review of v0.1.6 found the same thing built twice or built natively: a members sidebar next to a Room info panel that already had a Members tab; browser `<select>` and bare text inputs that ignored the design language; settings pages on phones for features phones do not have (keyboard shortcuts, a sidebar); a one-line cramped reply preview; bubble times that appeared on hover and moved the layout; unread counts that stayed on after a room was read. Users also asked to organise rooms with their own tags.

## Decisions
- **One implementation per thing (DRY).** Members live only in Room info > Members; the header people button opens that tab. Dropdowns are `Select` (our popup, built on the context-menu infrastructure: anchored menu on desktop, action sheet on phones), text boxes are `TextField`/`TextArea`. `controls-audit.test.ts` fails on a raw `<select>`, `<textarea>` or text `<input>` outside the shared components and a short allowlist.
- **Settings follow the platform.** `platform/capabilities.ts` derives what the running platform has (phone, Android, Tauri) and filters settings pages and controls: no Keyboard page or Enter-to-send on phones, no autostart on Android, wording and options that only exist on desktop stay on desktop.
- **Replies are quotes.** `ReplyQuote` shows the replied-to sender (full name, in their colour) and a two-line excerpt above the reply, in every layout; it jumps to and highlights the original when it is loaded.
- **Hover never changes layout.** Bubble times are always present; only decoration (background, toolbar overlay) changes on hover.
- **Read state.** A message counts as read when it is on screen (room opened, new message at the bottom, window returns). The room list shows the client-computed unread count and, after the user has read a room, ignores a stale server counter until something newer arrives (`stores/unread.ts`).
- **Custom room tags.** A tag is a standard Matrix room tag `u.<id>` (slug, interoperable with Element). Names, colours and order live in one account-data event `chat.alloy.room_tags`. Engine commands: `GetTagCatalog`, `SaveTagCatalog`, `SetRoomTag`, `DeleteRoomTag`; `RoomSummary.tags` carries the ids. UI: manage tags in Settings > Tags; toggle them from a room's menu; filter the room list by a tag; sort by activity, name, unread or tag (grouped by tag).

## Consequences
New dropdowns and text boxes must use the shared controls; new settings must declare which platforms have them. Tags set by other clients show up grey until adopted into the catalogue. Not done: tag assignment inside Room info, drag-and-drop ordering (up/down buttons today), per-room manual order inside a tag (the Matrix `order` field is not used yet).
