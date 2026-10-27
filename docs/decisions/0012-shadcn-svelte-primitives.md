---
id: adr-0012-shadcn-svelte-primitives
type: architecture-design
status: draft
title: "ADR 0012: shadcn-svelte primitives under alloy-theme (hybrid UI stack)"
parent: alloy-architecture
tags: [adr, ui, design]
---

# 0012: shadcn-svelte primitives under alloy-theme (hybrid UI stack)

- Status: accepted
- Date: 2026-10-27
- Source: maintainer review of hand-written primitive gaps (the update-download bar, `Toggle.svelte`) and the decision to adopt shadcn-svelte as an implementation layer while keeping alloy-theme as the single source of design truth.

## Context

Alloy's UI grew a set of hand-written primitives: switches, progress bars, dialogs, popovers, tooltips and menus. The business components on top of them — the timeline, composer, upload strip, media view, shell and settings pages — are intentionally bespoke. The primitives were not. They kept reimplementing the same hard problems: focus management, dismiss/escape handling, keyboard navigation, ARIA wiring and collision-aware popup positioning. The gaps were real, not theoretical: the update-download bar was `aria-hidden` with no progressbar semantics, and `Toggle.svelte` reimplemented `role=switch`/`aria-checked` by hand.

At the same time, ADR 0001 made `crates/alloy-theme` the single source of design truth. Every colour, radius and type decision lives in generated `--alloy-*` custom properties at `apps/alloy/ui/src/lib/theme/generated/themes.css`. Adopting a component library must not create a second design system.

## Decision

1. **`crates/alloy-theme` stays the single source of design truth.** The generated `--alloy-*` custom properties in `apps/alloy/ui/src/lib/theme/generated/themes.css` remain authoritative. shadcn-svelte does not introduce a competing palette, radius scale or type stack.
2. **shadcn-svelte is an implementation layer only.** It is adopted for behaviour-heavy primitives where the accessibility and interaction behaviour is the actual value. Bits UI, the headless library shadcn-svelte wraps, supplies that behaviour.
3. **Business components stay hand-written.** The timeline, composer, upload strip, media view, shell and settings pages keep their bespoke implementations. Only primitives are vendored.
4. **The bridge lives in `apps/alloy/ui/src/app.css`.** That file is the only place where shadcn's CSS variable contract meets alloy-theme (see "How the token bridge works" below).
5. **The hybrid stack is explicit.** Vendored primitives are allowed to use Tailwind utilities; hand-written business components keep using ADR 0001's CSS custom properties. The two meet at the token bridge, not at a second design language.

## Why

The behaviour that was being reimplemented by hand — focus management, dismiss/escape handling, keyboard navigation, ARIA wiring, collision-aware popup positioning — is exactly what Bits UI already maintains. Reimplementing it in-house meant carrying maintenance for interactions that have well-tested upstream implementations. The hand-written primitives also had real accessibility gaps: the update-download bar was `aria-hidden` with no progressbar semantics, and `Toggle.svelte` reimplemented `role=switch`/`aria-checked` by hand. Adopting the vendored primitives closes those gaps without asking Alloy to become an accessibility library.

## Excluded primitives

The following components are deliberately **not** replaced. Replacing them would mean re-implementing the same bespoke behaviour inside a wrapper, with no net gain:

- **`ContextMenu.svelte` (1,057 lines).** Drives the app-wide context menu *and* the mobile drill-in action sheet with its Back row and submenu search. This is product behaviour, not a generic primitive.
- **`Select.svelte`.** Deliberately delegates to that app-wide menu instead of owning a popup. A vendored select would fragment the single menu model.
- **`Modal.svelte`.** Owns backstack integration and the drag-to-dismiss bottom sheet on phones. Those are application-level concerns a generic dialog primitive does not model.
- **`TooltipLayer.svelte`.** Derives tooltips from `aria-label`/`data-tooltip` across the whole app. It is a global layer, not a per-component popover.
- **`Avatar.svelte`.** Uses the deterministic 12-token sender colour hashed from the Matrix user id (ADR 0001 §3). A generic avatar primitive would lose that identity contract.
- **`EmojiPicker`/`EmojiPickerHost`.** Product-specific data loading, recents and skin-tone behaviour.

## How the token bridge works

`apps/alloy/ui/src/app.css` imports `tailwindcss/theme.css` and `tailwindcss/utilities.css` into named cascade layers but deliberately **skips** `tailwindcss/preflight.css`. `src/lib/tokens.css` is already the authoritative unlayered reset. Importing Preflight would additionally rewrite `img, video { max-width: 100%; height: auto }` (regressing MediaView's fixed-size timeline media) and `a { color: inherit; text-decoration: inherit }` (regressing message-body links). Only the theme and utilities layers are imported.

The bridge aliases shadcn's variable contract — `--background`, `--primary`, `--border`, `--radius` and the rest — onto `--alloy-*` tokens. It also maps the radius scale 1:1 so `rounded-md` equals ADR 0001's 4px component radius rather than an arbitrary multiplier. No colour, radius or font literal is defined in the bridge itself.

## How dark mode works

shadcn expects a `.dark` class. Alloy themes select via the `data-theme` attribute on `<html>`: `alloy-light`, `alloy-dark`, `alloy-amoled`, plus custom themes. The bridge defines:

```css
@custom-variant dark (&:where(:root:not([data-theme="alloy-light"]) *));
```

so "not the light theme" covers every dark theme without enumerating them. Because each theme already resolves its own `--alloy-*` values, shadcn's `--background`-style variables follow the theme automatically. The variant only affects components that ship `dark:` utilities.

## Guard-test policy

The repo's static audits — `src/lib/emoji-guard.test.ts`, `src/lib/controls-audit.test.ts`, `src/lib/tooltip-audit.test.ts` and `src/lib/native-menu-audit.test.ts` — scan `src/lib/**`. Vendored files live in `src/lib/components/ui/**` and are therefore in scope.

**Rule:** vendored components must not be edited to satisfy an audit. Instead the audit's allowlist or scope is extended, and the decision to do so is recorded in the audit's comment. Vendored code stays byte-for-byte close to upstream except for the provenance header.

## Vendoring rule

Components are vendored from `https://shadcn-svelte.com/registry/<name>.json` and written to `src/lib/components/ui/<name>/`. A header comment records provenance and must not be removed. Restyle by passing `class`, never by editing the vendored file. `components.json` at `apps/alloy/ui/components.json` points the CLI at `src/app.css`, the `$lib` aliases and the `nova` style.

## Consequences

**Positive**

- Maintained ARIA, keyboard and focus behaviour comes from Bits UI instead of in-house code.
- Faster delivery of new primitives: vendoring a registry item is cheaper than reimplementing its interaction model.
- The update-download bar and `Toggle.svelte` gain correct semantics without bespoke accessibility work.

**Negative**

- A second styling system (Tailwind utilities in vendored primitives) must be kept coherent with ADR 0001's custom properties.
- Vendored code must be re-vendored on upstream upgrades; local edits are not allowed.
- Tailwind utilities now participate in the cascade and must not out-specify ADR 0001's unlayered CSS. Any specificity conflict is resolved in the bridge, not by editing vendored files.

## References

- `apps/alloy/ui/components.json`
- `apps/alloy/ui/src/app.css`
- `apps/alloy/ui/src/lib/utils.ts`
- `apps/alloy/ui/src/lib/components/ui/**`
- `apps/alloy/ui/vite.config.js`
