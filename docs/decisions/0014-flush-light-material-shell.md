---
id: adr-0014-flush-light-material-shell
type: architecture-design
status: accepted
title: "ADR 0014: flush, lightly transparent shell and read-first surfaces"
parent: alloy-architecture
depends-on:
  - adr-0001-ui-visual-language
  - adr-0002-discord-style-layout
  - adr-0012-shadcn-svelte-primitives
  - adr-0013-shadcn-chat-design-language
tags: [adr, design, ui, shell, material]
---

# 0014: flush, lightly transparent shell and read-first surfaces

- Status: accepted (2026-10-28)
- Issue: #105 (`type:decision`)
- Date: 2026-10-28
- Source: maintainer review of the 0013 conversation restyle. Two findings: the shell still read as
  a stack of frosted cards with too much padding around a chat that should be the product, and the
  details panel still presented identity as a form to fill rather than text to read.

## Context

0013 moved the conversation surface to a soft, bordered chat language. The shell around it did not
follow. Panes were individually glazed with the floating-layer recipe (28-44px blur, 48-84% fill,
specular sheen, large radii, drop shadows), the window was padded so each pane floated as a card,
and the conversation lost width to chrome. That reads as a demo of glass, not as a tool: the chat is
the product and everything else is navigation for it.

The room details panel had the mirror-image problem at the interaction level. Name, topic and private
note were permanently editable form fields. Most visits to the panel are read-only (who is in the
room, what it is about, shared media), yet every visit looked like an edit screen, and the fields
competed with the information the panel exists to show.

The maintainer asked for two changes:

1. A lightly transparent shell — "not liquid glass" — with far less padding and more room for chat.
2. Read-first identity: name and topic are text; a pencil appears on hover, and only then does an
   input replace the text.

## Decision

### 1. Material model: four depths, one glass recipe

Every surface belongs to one of four layers. Only the fourth is genuinely frosted.

1. **Canvas.** A chromatic aurora over the theme canvas, with a fine monochrome grain to hide
   banding (`--alloy-canvas-art`, `--alloy-grain`). It is the only place colour "moves".
2. **Chrome.** The rail, sidebar, details panel and thread column are *lightly* transparent:
   `--alloy-chrome-surface` (86% panel) for navigation, `--alloy-chrome-surface-strong` (90%) for
   side panes, both with `--alloy-chrome-filter: blur(26px) saturate(160%)`. No radius, no shadow,
   no sheen — a pane, not a card.
3. **Conversation.** Near-opaque: `--alloy-chat-surface` (92% canvas) with
   `--alloy-chat-filter: blur(30px) saturate(140%)`. Reading must not depend on what drifts behind.
4. **Floating layers.** Menus, dialogs, pickers, the command palette and composer popovers keep the
   full glass recipe (`--alloy-glass-*`): the only things that float are the only things that are
   frosted. The glass is **flat**: fill, rim, blur and a soft drop shadow, with no specular sheet
   (`--alloy-glass-sheen` is transparent) and no top-edge catch (`--alloy-glass-highlight` is a
   zero shadow). The own bubble and the send button are flat accent fills for the same reason —
   gloss reads as plastic, most of all on a dark canvas. The dark canvas also uses two blooms at
   roughly half strength instead of four bright ones, so the aurora stays a wash and not a sheen.

A light theme needs slightly more chrome opacity than a dark one; the 86/90% split exists so
secondary 12px text still clears 4.5:1 over the blooms. The dark theme keeps the same recipe with
more canvas showing — 84% chat, 80/86% chrome, set under `:root:not([data-theme='alloy-light'])` —
because 92% over a near-black canvas reads as a void rather than a pane. Its palette is lifted one
step as well (`packages/themes/alloy-dark.json`), so the conversation is lighter than the chrome
instead of darker, and Pure black stays a visibly different choice.

### 2. Flush geometry

The shell has no outer padding and no pane cards. Panes meet at hairline separators
(`--alloy-chat-rim`: text at 8%), and the window edge is the pane edge. Dimensions
(`src/lib/layout.css`, `LAYOUT_LIMITS`):

| Part | Value |
|---|---|
| Icon rail | 56px (`--alloy-layout-rail-width`) |
| Sidebar | 200–320px, default 240, user-resizable |
| Details panel | 320–420px, default 380, user-resizable |
| Header | 52px (`--alloy-layout-header-height`) |
| Shell gutter | 12px phone / 16px desktop (`--alloy-layout-gutter`) |
| Section gap | 16px (`--alloy-layout-section-gap`) |

Depth comes from the material layers and hairlines, never from stacked elevation. This supersedes
the floating-pane geometry of 0002 while keeping its column order and drawer/sheet contract.

### 3. Density type scale

Chrome uses four sizes with distinct jobs instead of one flat weight
(`apps/alloy/ui/src/lib/tokens.css`):

| Token | Value | Job |
|---|---|---|
| `--alloy-font-size-title` | 15px | panel and dialog titles |
| `--alloy-font-size-row` | 13px | room names, list rows, body values in panels |
| `--alloy-font-size-preview` | 12px | message previews, secondary panel text |
| `--alloy-font-size-meta` | 10.5px | tracked section headings, counts, timestamps |
| `--alloy-letter-spacing-caps` | 0.08em | uppercase section headings |

### 4. Stacked reply bubble

In the bubble layout a quoted message is **its own bubble above the reply**, not a panel inside it
(`ReplyQuote.svelte` `stacked`). The two bubbles sit flush — their two 1px edges read as one seam —
and the junction is square on both sides: the quote keeps its 20px outer top corners and the reply
keeps its bottom corners including the sender-side tail, so the pair reads as one card with a
coloured header. The quote keeps the original sender's colour as a 3px leading bar and shows that
sender's name in the same colour; on
own messages the header is a lighter plate of the own-bubble blue. Inline quotes remain for the
`modern` and `irc` layouts, where a stack would break the row model.

The sender side is the avatar side — left for received messages, right for own ones. Its corners
tighten to `--alloy-bubble-radius-tail` (8px) on every message: the bottom corner always, and the top
corner too when the message joins a run from the same sender. A lone message therefore shows the tail,
and a run reads as one column rooted at that person's avatar instead of a stack of separately rounded
cards. Attachments that fill the message (a bare picture, video, sticker or album) are message
surfaces too, so they take the same 20px radius and tail; cards nested inside a bubble use
`--alloy-bubble-radius-nested` (12px).

### 5. Read-first panels

Property editing in panels follows one pattern:

- **Read by default.** The value is text, at `--alloy-font-size-row`, in the same place whether or
  not the user may edit it.
- **Pencil on hover.** A 24px icon button is revealed on `:hover` and `:focus-within` of the value
  (`.editable:hover .edit-btn`). It is always visible on touch (`@media (hover: none)`) and when the
  value is empty, so the affordance is never hover-only in practice.
- **Click to edit.** Only then does a `TextField`/`TextArea` replace the text, focused.
- **Keyboard parity.** Enter saves, Escape cancels, blur saves; focus returns to the pencil after a
  keyboard edit so the tab order stays put.
- **Drafts never race.** A closed field follows store updates; an open field is never overwritten
  mid-typing.

This pattern owns room name, topic and private note in `RoomInfoPanel.svelte` and is the default for
future editable properties. Panels are not forms.

### 6. Tokens stay authoritative

Every value above is a token or a `color-mix()` of tokens in `tokens.css` / `layout.css`; components
hold no colour, radius or spacing literals. The chrome and chat materials have explicit fallbacks:

- `@media (prefers-reduced-transparency: reduce)` and
  `@supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px)))` set the
  chrome and chat filters to `none` and their surfaces to the opaque panel/canvas tokens, and drop
  the canvas art. Layout and hierarchy survive without transparency.

## Token changes

All additive in `apps/alloy/ui/src/lib/tokens.css` and `layout.css`; no theme-schema change is
required, because every material derives from existing theme tokens.

| Token | Value |
|---|---|
| `--alloy-chrome-surface` | panel at 86% (80% dark) |
| `--alloy-chrome-surface-strong` | panel at 90% (86% dark) |
| `--alloy-chrome-filter` | `blur(26px) saturate(160%)` |
| `--alloy-chat-surface` | canvas at 92% (84% dark) |
| `--alloy-chat-filter` | `blur(30px) saturate(140%)` |
| `--alloy-chat-rim` | text at 8% |
| `--alloy-bubble-radius` | 20px |
| `--alloy-bubble-radius-tail` | 8px |
| `--alloy-bubble-radius-nested` | 12px |
| `--alloy-bubble-max-width` | 72% (85% on phones) |
| `--alloy-font-size-title/row/preview/meta` | 15 / 13 / 12 / 10.5px |
| `--alloy-letter-spacing-caps` | 0.08em |

## What this supersedes

- **ADR 0001** items 4 (bubble radii and inline reply panel), 6 (composer rhythm) and 8 (flat 4px
  component scale) are superseded *for the conversation surface and shell* by this record. Its
  non-visual rules — functional icons, one icon set, sender colours, theme toggle, chat background,
  "less chrome" — remain in force.
- **ADR 0002**'s shell material and geometry (floating panes, 68px rail, 240px fixed sidebar,
  320px details panel, flat opaque panels) is superseded by §2. Its four-column order, resizable
  panels and mobile drawer/sheet contract remain.
- **ADR 0013** is superseded: its bubble radius (`--alloy-radii-xl`, 12px) and its "shell panels stay
  flat, softness is confined to the conversation" boundary are replaced by the material model above.
  What survives from 0013: the shadcn chat anatomy, the composer field at `--alloy-radii-2xl`
  (16px), border-over-shadow for bubbles, tokenised rhythm, and the scroll engine untouched.
- **ADR 0005**'s mobile shell behaviour (drawers, sheets, tab bar) is unchanged.
- **ADR 0012**'s boundary is unchanged: vendored primitives are behaviour-only; business components
  stay hand-written.

## Alternatives considered

- **Keep the per-pane glass card.** Visually rich, but it costs width twice (padding around the
  card, radius inside it) and makes the chat look like one widget among several. Rejected by
  maintainer preference.
- **Fully opaque shell.** Cheapest for contrast, but the aurora disappears entirely and the app
  loses the material hierarchy that separates chrome from conversation. Rejected.
- **Keep name/topic as always-editable fields.** Fewer states, but makes a read surface look like a
  settings form and competes with members and media. Rejected by maintainer preference.
- **Show the pencil permanently.** Discoverable, but adds a control to every property row and
  restores the form feel. Rejected; touch gets a permanent pencil instead.
- **Stack replies in every layout.** Breaks the `modern`/`irc` row model, where there is no bubble to
  stack against. Rejected.

## Consequences

- The conversation gains the width the padding used to take at every breakpoint; screenshots at
  1280 / 900 / 390 are the review artifact.
- Transparency is a contrast risk: secondary text and hairline-only boundaries must be re-checked per
  theme, including the light theme over the brightest bloom. The reduced-transparency and
  no-backdrop-filter fallbacks are part of the contract, not a nicety.
- The phone E2E audit must cover the stacked reply seam and the touch-visible pencils in the details
  panel, not only the desktop hover path.
- `RoomInfoPanel.svelte` now owns three edit sessions (name, topic, note) with focus-return logic;
  these need tests alongside the existing room-info store tests.
- A design-token audit that fails on literal colours, radii and spacing in `apps/alloy/ui/src/lib/**`
  remains the follow-up that keeps this language from drifting.

## Rollout

1. Tokens: material layers, density scale, bubble radii and reply-stack tokens; fallbacks.
2. Shell: flush geometry, hairline separators, rail/sidebar/details widths.
3. Conversation: stacked reply bubble and seam corners.
4. Details panel: read-first identity and note, hover pencils, keyboard parity.
5. Docs: supersede 0001/0002/0013 visual values, update `apps/alloy/ui/SPEC.md`.

## References

- ADR 0001 — UI visual language v1
- ADR 0002 — Discord-style layout
- ADR 0005 — mobile-first shell
- ADR 0012 — shadcn-svelte primitives
- ADR 0013 — shadcn chat design language
- `apps/alloy/ui/src/lib/tokens.css`, `src/lib/layout.css`
- `apps/alloy/ui/src/lib/components/shell/AdaptiveShell.svelte`
- `apps/alloy/ui/src/lib/components/shell/RoomInfoPanel.svelte`
- `apps/alloy/ui/src/lib/components/timeline/MessageRow.svelte`, `ReplyQuote.svelte`
