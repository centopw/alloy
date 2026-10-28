---
id: adr-0013-shadcn-chat-design-language
type: architecture-design
status: superseded
title: "ADR 0013: shadcn chat design language for the conversation surface"
parent: alloy-architecture
depends-on:
  - adr-0001-ui-visual-language
  - adr-0002-discord-style-layout
  - adr-0012-shadcn-svelte-primitives
tags: [adr, design, ui, chat]
---

# 0013: shadcn chat design language for the conversation surface

- Status: superseded by 0014
- Superseded: 0014 replaces this record's bubble radius (12px), its border-only bubble surface
  and its "shell panels stay flat and opaque" boundary with the material model and the stacked
  reply bubble. The anatomy, the 16px composer field, the tokenised rhythm and the untouched
  scroll engine remain in force.
- Issue: #105 (`type:decision`)
- Date: 2026-10-27
- Source: maintainer review of the shadcn/ui chat components (MessageScroller, Message,
  Bubble, Attachment, Marker, PromptInput). The current conversation surface reads as generic
  next to it.

## Context

ADR 0001 pinned the visual language to "Telegram-inspired, text-first": flat message rows,
4px components, a 6px composer field, and no borders where spacing suffices. ADR 0012 then
adopted shadcn-svelte as an *implementation layer for behaviour-heavy primitives only*, and
explicitly kept the timeline, composer, upload strip, media view, shell and settings
hand-written on `alloy-theme` tokens.

The maintainer's goal is the opposite half of that decision: the *look* of the shadcn chat
demo, which reads as modern and deliberate rather than generic. shadcn's chat components are
React, so what ports is not code but anatomy and visual values. Two consequences follow:

1. The change is a visual-language decision, not a dependency migration. It must be recorded
   as such, and it supersedes the specific visual values of ADR 0001 rather than silently
   diverging from them.
2. The boundary in ADR 0012 still holds in one direction: business components stay
   hand-written, and `crates/alloy-theme` stays the single source of design truth. Adopting a
   design language does not mean vendoring React components or a second palette.

## Decision

Adopt the shadcn/ui chat *anatomy and visual values* on Alloy's hand-written conversation
surface, while keeping Alloy's own message grouping model.

1. **Anatomy.** The conversation surface is organized as `MessageScroller` (the existing
   scroll engine), `Message` (row: avatar · header · body · footer), `Bubble` (the only
   surface), `Marker` (date separators), `Attachment` (media cards) and `PromptInput` (the
   composer). This is a naming and responsibility model for the existing components, not a
   new component library.
2. **Soft radii.** Bubbles and attachment cards use `--alloy-radii-xl` (12px); the composer
   field and primary input surfaces use `--alloy-radii-2xl` (16px). The joined/tail corner of
   a grouped bubble keeps `--alloy-radii-sm` (4px). Shell panels stay flat (0px, per ADR
   0002) — the softness is confined to the conversation surface.
3. **Keep Alloy's grouping.** Own messages stay right-aligned, others left-aligned, and
   consecutive messages from one sender stay grouped with a small radius on the joined
   corner. shadcn's flatter, non-tailed alignment is deliberately **not** adopted.
4. **Border over shadow.** Surfaces are defined by a 1px `--alloy-color-border-subtle` edge
   and a muted fill, not by elevation. Drop shadows are reserved for floating layers
   (menus, dialogs), which already use `--alloy-color-border-floating`.
5. **Rhythm.** Bubble padding is 8px × 12px (scale-2 × scale-3); the gap inside a sender
   group is 2px; the gap between groups is 12px (`--alloy-spacing-scale-3`); the grouped
   avatar is 32px. These become layout tokens, not per-component literals.
6. **Type scale.** Body stays 14px (`--alloy-typography-size-base`); message meta (time,
   ticks) is 11px (`--alloy-typography-size-xs`) and `--alloy-color-text-secondary`; sender
   names are 12px (`--alloy-typography-size-sm`), semibold.
7. **Bubble width.** A bubble caps at 72% of the conversation width on desktop and 85% on
   phones.
8. **`alloy-theme` stays authoritative.** Every value above is a token. No component may
   hardcode a colour, radius or spacing literal; the existing static audits enforce this.
9. **The scroll engine is not replaced.** `MessageScroller` behaviour (paging, anchoring,
   prepend) is already custom and was verified in the 0.2.7 E2E run. This record restyles it;
   it does not reimplement it.

## Token changes

Add to `crates/alloy-theme` (`schema.rs` `RadiiTokens`, then every theme definition and the
regenerated `apps/alloy/ui/src/lib/theme/generated/themes.css`):

| Token | Value | Replaces |
|---|---|---|
| `--alloy-radii-xl` | 12px | new; bubbles, attachment cards |
| `--alloy-radii-2xl` | 16px | new; composer field |

Already present and reused: `--alloy-color-border-subtle`, `--alloy-radii-sm`,
`--alloy-radii-md`, `--alloy-radii-lg`, `--alloy-spacing-scale-*`,
`--alloy-typography-size-*`.

Reader-facing aliases in `tokens.css`: `--alloy-radius-xl`, `--alloy-radius-2xl`, and
`--alloy-spacing-bubble-x/y`, `--alloy-bubble-max-width` / `--alloy-bubble-max-width-phone`.

## What this supersedes

- **ADR 0001** items 4 (bubble radii), 6 (composer as a "single rounded field" at the old
  radius) and 8 (flat/4px component scale) are superseded *for the conversation surface* by
  this record. ADR 0001's non-visual rules — functional icons, one icon set, sender colours,
  theme toggle, chat background, "less chrome" — remain in force.
- **ADR 0002**'s shell geometry (four columns, 68px rail, 240px sidebar, 320px details panel,
  flat panels) is unchanged.
- **ADR 0012**'s boundary is unchanged: vendored primitives are behaviour-only; business
  components stay hand-written. This record only says those hand-written components adopt a
  sharper visual language.

## Alternatives considered

- **Vendor shadcn's React chat components.** Impossible directly (React), and a Svelte port
  would replace the custom scroll engine and the ADR 0005 mobile shell — the two things most
  likely to regress. Rejected.
- **Full shadcn-svelte block rebuild of the shell/composer/timeline.** Conflicts with ADR
  0002's mobile drawer/sheet contract and discards the verified paging/anchoring work.
  Rejected.
- **Keep ADR 0001's flat 4–8px scale and only change colours.** Cheapest, but radii and
  rhythm are what make the demo feel modern; the result would stay generic. Rejected.
- **Adopt shadcn's flat alignment (no Telegram tails).** Loses grouping identity that ADR
  0001 deliberately added. Rejected by maintainer preference.

## Consequences

- A visible change to the conversation surface across every theme; before/after screenshots
  at 1280 / 900 / 390 are required for review.
- Bigger radii and padding can reintroduce overflow and sub-44px tap targets — exactly what
  failed CI before 0.2.7. The **mobile E2E audit must run after each phase**, not once at the
  end.
- Theme schema change ripples into generated CSS and Rust tests; do it first, in isolation,
  and run `cargo test -p alloy-theme` plus `pnpm gen:themes`.
- A new `type:decision` tracking issue is needed for the ADR reference.
- Follow-up: a design-token audit that fails on literal colours/radii in `src/lib/**`, so the
  language cannot drift the way it did before.

## Rollout

1. Tokens: theme schema + `xl`/`2xl` radii + regenerated CSS + token audit.
2. Bubbles: `MessageRow.svelte` restyle only (no paging/scroll logic).
3. Markers and attachments: date separators + `MediaView`.
4. Composer: `PromptInput`-style field in `Composer.svelte`.
5. Shell cohesion: sidebar/settings surfaces aligned so chat does not look pasted in.
6. Docs: mark ADR 0001 supersessions, note 0012, update `apps/alloy/ui/SPEC.md`.

## References

- ADR 0001 — UI visual language v1
- ADR 0002 — Discord-style layout
- ADR 0012 — shadcn-svelte primitives
- `crates/alloy-theme/src/schema.rs`, `src/builtin.rs`, `src/bin/gen-css.rs`
- `apps/alloy/ui/src/lib/theme/generated/themes.css`
- `apps/alloy/ui/src/lib/components/timeline/MessageRow.svelte`
- `apps/alloy/ui/src/lib/components/composer/Composer.svelte`
- `apps/alloy/ui/src/lib/components/timeline/MediaView.svelte`
