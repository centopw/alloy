# 0002: Discord-style layout, rounded surfaces, settings in a menu

- Status: accepted (supersedes items 2, 4, 5 and 6 of 0001; keeps 1 (no emoji), 3 (per-user colours), 7, 8)
- Date: 2026-10-17
- Source: maintainer direction with a Discord screenshot as the reference.

## Context
0001 chose a Telegram-style conversation surface (bubbles, chat-list rows, theme controls near the chat). The maintainer prefers Discord's design language: a persistent multi-column layout, flat message rows with avatars, one consistently rounded shape language, and every setting that is not used daily moved into a settings menu so the chat surface stays clean.

## Decision
1. **Four-column shell (desktop).**
   - Rail (72 px): a "home" entry (all rooms and direct messages), one round avatar per Matrix space and a "join/create" button. The active entry shows a left pill indicator and a squarer (rounded-rectangle) shape; inactive ones are circles.
   - Sidebar (240 px): space/home name header, search field, grouped room list (Invites, Rooms, Direct messages). A row is room avatar + name, with a count badge when unread; no previews, no timestamps. Encrypted rooms show a small lock glyph.
   - Conversation: header (room name, topic, member-list toggle, search), message list, composer.
   - Members panel (240 px, toggleable, shown by default on wide windows): grouped by role (Admin, Moderator, Members, Invited), replaces the details drawer from #67. Room topic and encryption state sit on top of this panel.
   - Narrow layouts (< 900 px) collapse to one column with a drawer for rail+sidebar and a sheet for members, as before.
2. **User panel** pinned to the bottom of the sidebar: own avatar, display name, user id/status line and a settings gear. It is the only entry point for account/app settings besides the command palette.
3. **Settings menu** (full-screen overlay, left nav + content, Esc closes). Sections: My Account (profile, devices, sign out), Appearance (light/dark toggle, pure black, follow system, theme package, density, font size, message spacing), Security (recovery key, verification), Notifications, Advanced (developer HUD, cache, logs, transport), About. Nothing from these sections appears in the chat chrome. The theme toggle (a switch, per 0001 item 2) lives only in Appearance.
4. **Messages are flat rows, not bubbles.** 40 px round avatar on the first message of a group, coloured sender name (0001 item 3) + timestamp on one line, body below; follow-up messages within 7 minutes by one sender are compact rows (hover reveals the time in the gutter). Hover shows a floating rounded action bar (react, reply, edit, delete, more). Reactions are rounded pills under the body. Replies show a connected quote line above the message. Code blocks and quotes are rounded surfaces. Own messages are not right-aligned. The bubble/gutter option from 0001 is removed.
5. **Avatars everywhere.** Users, rooms and spaces always show an avatar: the Matrix avatar image (fetched via `GetMedia` thumbnail, cached) when present, otherwise coloured initials using the per-user colour tokens. Shape: circle for users and spaces in the rail, rounded square for rooms.
6. **One shape language.** All surfaces are rounded using a token scale: `--alloy-radius-sm` 4 px (inline code, tags), `-md` 8 px (inputs, buttons, list rows), `-lg` 16 px (cards, popovers, message action bar, code blocks, dialogs, settings panel, composer), `-full` (avatars, badges, reaction pills). The theme schema already carries these four tokens; the `lg` value moves from 12 px to 16 px. A nested element's radius is at most its parent's radius minus the padding between them. Hard-coded radii are not allowed in components.
7. **Composer** is one rounded field: attach (+) on the left, text, emoji on the right; send is Enter. No toolbar row.
8. **Icons.** Still no emoji in chrome. The Lucide allowlist may grow with functional glyphs the layout needs (hash for rooms, lock, users, settings, plus, search, bell, pin). Controls keep `aria-label`.
9. **Keep daily controls only on the main surface**: search, new room, mention/reply/react/send, members toggle, unread badges. Everything else belongs to the settings menu or the room's context menu.

## Consequences
- Tokens: add `--alloy-radius-*` and sidebar/rail surface tokens to the theme crate (additive, schema v0 compatible); regenerate CSS with `gen-css`.
- UI work: `AdaptiveShell`, `RoomList`, `Timeline`, `MessageRow`, `Composer`, `SettingsDialog`, `RoomDetails` (becomes the members panel) are reworked; new `SpaceRail`, `UserPanel`, `Sidebar`, `MembersPanel`. The room-list filter chips of 0001 item 5 are replaced by sections and search.
- The emoji-guard test and the colour/AA requirements stay. Reviewers reject hard-coded radii and hex colours.
- The members, mention, upload-progress and media-viewer work of #66-#68 continues and plugs into this layout.
