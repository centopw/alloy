---
id: alloy-ui
type: submodule-design
status: draft
title: alloy-ui — Svelte 5 view layer
parent: alloy-app
tags: [svelte, ui, view]
depends-on: [alloy-protocol]
---

## Responsibility

The Svelte 5 / TypeScript / Vite view layer. It consumes generated protocol types over a `Transport` (Tauri IPC in the app, `MockTransport` in development, the WS bridge for browser E2E), owns the runes stores that apply `VectorDiff` streams, and renders the shell, timeline, composer, settings, onboarding, tags and update surfaces. It also owns the theme store that consumes generated CSS variables, the platform-capability layer, and the audit tests (controls, native menus, tooltips, emoji guard). Below 900 px it renders the phone shell.

## Boundary

It talks only to the protocol through `ipc/`; it has no Rust crate dependencies, makes no homeserver calls and holds no product logic. Platform differences live in `platform/`, native controls are wrapped in shared components, and design tokens arrive as CSS variables from `alloy-theme`. It must not call matrix-sdk or bypass the engine. Pull-to-refresh and content-URI picking are wired through `platform/files.ts` and the shell's `pullToRefresh` action; the composer's tiered attach menu calls `pickMediaFiles` for pictures and videos and `pickFiles` for a plain file, with polls and locations under its More flyout. Staged pictures and videos each carry a thumbnail in the preview strip — a picture's image or a video's poster frame — and fall back to a file-type glyph when no preview can be drawn.

### UI stack

Interaction-heavy primitives are vendored from shadcn-svelte (Bits UI behaviour) into `src/lib/components/ui/**`; business surfaces — shell, timeline, composer, upload strip, media view, settings — stay hand-written. `crates/alloy-theme` remains the single source of design truth: `src/app.css` aliases shadcn's CSS variable contract onto the generated `--alloy-*` tokens, imports Tailwind's theme and utilities layers without Preflight (so `tokens.css` stays the authoritative reset), and keys dark mode off the `data-theme` attribute rather than a `.dark` class. Vendored files are never edited to satisfy a repo audit or to restyle; overrides go through the component's `class` prop. The deliberately excluded primitives (`ContextMenu`, `Select`, `Modal`, `TooltipLayer`, `Avatar`, `EmojiPicker`) encode product behaviour — the app-wide menu model, backstack integration and drag-to-dismiss sheets, global tooltip derivation, deterministic sender colours — that a generic primitive does not model. See decision 0012.

Known limitations: the accessibility audit is pending (#20, #36); user-supplied keybinding and slash-command JSON packages are not loaded yet; only `switch` and `progress` are vendored so far, so the migration is incremental and most primitives remain hand-written.

### Shell material

The shell is flush: no outer padding, no pane cards, panes joined by hairlines
(`--alloy-chat-rim`). Depth is four material layers, not elevation (decision 0014). The canvas is a
chromatic aurora with grain (`--alloy-canvas-art`); navigation and side panes are lightly
transparent (`--alloy-chrome-surface` 86%, `--alloy-chrome-surface-strong` 90%,
`--alloy-chrome-filter` 26px blur); the conversation is near-opaque (`--alloy-chat-surface` 92%,
`--alloy-chat-filter` 30px blur); only floating layers (menus, dialogs, pickers, composer popovers)
keep the full `--alloy-glass-*` recipe — flat fill, rim and soft shadow, with no specular sheet or
top-edge catch. Dark themes draw more of the canvas (84% chat, 80/86% chrome) over a two-bloom,
lower-saturation aurora, so the panes read as layers rather than a black void and the wash never
looks like gloss. Reduced-transparency and no-`backdrop-filter` environments
switch the chrome and chat filters off and their surfaces to the opaque tokens; layout and hierarchy
must survive that. Geometry (`src/lib/layout.css`, `LAYOUT_LIMITS`): 56px rail, sidebar 200–320
(default 240), details 320–420 (default 380), 52px header. Chrome type uses
`--alloy-font-size-title` (15px), `-row` (13px), `-preview` (12px) and `-meta` (10.5px) with
`--alloy-letter-spacing-caps` for uppercase headings.

### Conversation surface

The chat keeps the shadcn chat *anatomy* without vendoring its React components (decision 0013), and
its visual values now follow decision 0014. Bubbles are the default layout on every device; `modern`
(flat rows) and `irc` remain choices in Settings > Appearance. Bubbles use
`--alloy-bubble-radius` (20px), the sender-side tail `--alloy-bubble-radius-tail` (8px), and cap at
`--alloy-bubble-max-width` (72%, 85% on phones). The sender side is the avatar side: left for received
messages, right for own ones. Its corners tighten to the tail radius on every message, so a lone
message shows the tail and a run from one sender joins into one column; bare pictures and albums are
message surfaces too, so they take the same radius and tail. Cards nested inside a bubble (pictures,
files, link and location cards, poll answers) use `--alloy-bubble-radius-nested` (12px) and never
compete with the message surface around them. `modern` and `irc` draw no bubble, so a bare picture is
the message surface there: it takes the 20px radius with no tail, because the avatar gutter already
names the sender and there is no bubble corner to shape. Own bubbles are the one saturated surface; received
bubbles use `--alloy-bubble-other-surface` over a 1px text-at-8% edge. Rhythm is tokenised
(`--alloy-spacing-bubble-x/y`, `--alloy-avatar-grouped-size`), message meta is the xs token and sender
names are `--alloy-font-size-preview`, semibold; date separators are centred pills. **Replies stack:**
in the bubble layout a replied message is a header, a quote card and the reply bubble — three
separately rounded surfaces with a 4px gap, each hugging its content and sharing only the sender
edge (right for own, left for received), so a two-character reply beside a long quote cannot open a
notch. The header reads `Reply to <Name>` above the cards; the quote card is `ReplyQuote` `stacked`
(the name is not repeated inside it), takes the full 20px radius and carries the quoted sender's
colour as a 2px inset spine, clamped to three lines. In a group the quote card is the first surface,
so it takes the 8px tail on the outer corner and the bubble below keeps its own radius. Inline
quotes remain for `modern` and `irc`. **A touch rule may raise a card's tap target and add outer
spacing; it must never re-declare shorthand `padding`.** At ≤899px the interactive rule makes the
card 44px tall, and a shorthand there resets the quote card's asymmetric left inset, which paints
the 2px spine over the first glyph of the excerpt — the defect the 390 walk caught. The scroll
engine (paging, anchoring, prepend) is untouched.

### Room list

A room's tags are the account's, not the install's: the catalogue is the synced `chat.alloy.room_tags`
account-data event and each assignment is a `u.<tag>` room tag, so a reinstall, a restart or an update
finds the same groups. The list shows one chip per tag that any room carries (catalogue order, then
anything a room still references), and a chip filters the list to that tag. Beside the chips, **Group**
turns the list into one section per tag, untagged rooms last; it is the same `roomSort` preference as
"By tag" in the sort menu, so the two controls always agree. Chips and the grouping control are
present on every layout including the phone — a phone that showed tags but no way to group them was
the reported defect — and they use the same quiet chip, 44px tall on touch.

### Radius scale

One scale for the whole interface: each step is a role, not a size, and a surface takes the step that
matches its job. Two roles may share a value when they never appear in the same plane (the 12px
nested message card and the 12px control inside a floating layer); a role may not have two values.
The conversation steps are owned by decision 0014 (`--alloy-bubble-*`); the rest use the component and
glass tokens in `tokens.css`.

| Step | Token | Job | Surfaces |
|---|---|---|---|
| 20px | `--alloy-bubble-radius` | message surface | text bubble; bare picture, video, sticker, album |
| 8px | `--alloy-bubble-radius-tail` | sender-side corners | every message on the avatar side: bottom always, top when joined to a run |
| 12px | `--alloy-bubble-radius-nested` | card inside a message | picture frame, file tile, link preview, location card, poll answer |
| 16px | `--alloy-radii-2xl` | composer surface | composer field, staged attachment strip, composer banner |
| 26px | `--alloy-glass-radius-shell` | floating shell | dialog, settings, command palette, onboarding, mobile tab bar |
| 18px | `--alloy-glass-radius-panel` | floating panel | context menu, mention/emoji popover and sheet top, profile card, action sheet |
| 12px | `--alloy-glass-radius-control` | control in a floating layer | message action bar, format/upload strip, settings nav item, side-panel inset |
| 8px | `--alloy-radii-lg` | card or row | settings group, radio card, room-info card and rows, sidebar row, search and filter tabs, rail tile, menu row, list row, notice, thumbnail |
| 6px | `--alloy-radii-md` | control | button, text field/area, select, segmented outer, small icon button, chip; a message row's hover band and the menu spotlight hole (they frame the row, not the bubble) |
| 4px | `--alloy-radii-sm` | inline mark | tooltip, keyboard key, inline code, search hit, picker tab and cell, inline reply quote |
| 0 | — | flush junction | docked phone panel, narrow nav, composer attached state, quote-to-reply seam |
| full | `--alloy-radii-full` | round affordance | avatar, badge, tag, dot, circular button |

The 8px tail and the 12px nested card are deliberately distinct: the tail is an outer corner of the
message surface against the avatar side; the nested card is inset 10–14px inside that surface. They
never share a plane, so the four-pixel difference does not read as drift; unifying them would either
erase the tail into the 20px bubble or pinch every attachment.

**Normalization queue** (the scale above is the target; these are the places the tree still differs):

| Area | Selector (file) | Now → target |
|---|---|---|
| Composer field | `.field` (`Composer.svelte`) | 12 → 16 |
| Context menu shell | `.context-menu` (`ContextMenu.svelte`) | 16 → 18 |
| Menu rows | `.item`, `.sheet .item` (`ContextMenu.svelte`); `.option` (`MentionPicker.svelte`); `button` (`CommandPalette.svelte`) | 12/6/6/6 → 8 |
| Sheet tops | `.context-menu.sheet`, `.host.sheet` (`ProfileCardHost.svelte`), `.sheet` (`VerificationDialog.svelte`), `.dialog` phone top (`Modal.svelte`) | 8/8/8/16 → 18 |
| Mention popover | `.picker` (`MentionPicker.svelte`) | 12 → 18 |
| Rows and cards | `.row` (`Tags.svelte`), `.list` (`PermissionsPanel.svelte`), `.results li` (`DirectoryDialog.svelte`), `.media-cell`, `.notice` (`RoomInfoPanel.svelte`), `.choice` (`SettingRadioMatrix.svelte`), `.notice` (`UpdateNotice.svelte`), `.swipe` (`RoomSwipe.svelte`), `.picture img/video`, `.picture-icon` (`Composer.svelte`), `pre` (`ChangelogDialog.svelte`) | 6 → 8 |
| Rail tile | `.space` (`SpaceRail.svelte`) | 12 → 8 |
| Action bar | `.toolbar` (`MessageRow.svelte`) | 16 → 12 |
| Legacy aliases | `SpaceChips`, `MobileTabBar`, `Badge`, `UpdateNotice` | `--alloy-radius-*` → `--alloy-radii-*` (no visual change) |

No other radius value is permitted; `0` is a junction, not a size. Values that are not in the table
are a review finding.

### Settings surfaces

A settings group is one plate; its rows are separated by hairlines and nothing inside the group draws
a second outline. A choice that shows a preview (Settings > Appearance, message layout;
`SettingRadioCards`) is an even grid of tiles — three across when the column allows, one column below
36rem, so the last option never sits alone in a half-empty row. The tile *is* the chat canvas, so the
preview shows the real bubble contrast without a frame inside a card. Selection is the accent ring
plus a check in the footer and never repaints the surface, because a tint would wash out the preview
it is meant to show; the check slot is always reserved, so labels keep their width in every state.

### Room details panel

Read-first, not a form (decision 0014). Identity is a horizontal 56px avatar with the room name,
alias and meta beside it; name, topic and private note render as text and reveal a 24px pencil on
hover/focus-within (always visible on touch and when the value is empty). Clicking the pencil swaps
the text for a focused `TextField`/`TextArea`; Enter saves, Escape cancels, blur saves, and focus
returns to the pencil after keyboard edits. Open fields are never overwritten by store updates;
closed fields follow the room. Sections are separated by hairlines and use the
`--alloy-font-size-meta` uppercase heading; rows and cards are 44px-minimum with
`--alloy-radii-lg` surfaces. The Shared media switch hugs its labels instead of stretching across the
panel. A thumbnail is best-effort: when the engine refuses one the tile requests the original file
once and shows that, and when the webview rejects the picture too the retry falls back to the kind
icon instead of the browser's broken-image placeholder. Only a served URL is cached, a refusal is
retried on the next lookup, and the cache lives for one panel, so reopening Room info retries media
that failed earlier. Save errors surface inline; a private
note is the user's own room account
data (synced to their account, never visible to other members).

