---
id: adr-0015-a11y-contrast-tokens
type: architecture-design
status: accepted
title: "ADR 0015: accessibility contrast tokens for text, badges, avatars and the theme checker"
parent: alloy-architecture
depends-on:
  - adr-0001-ui-visual-language
  - adr-0014-flush-light-material-shell
tags: [adr, design, ui, accessibility, theme]
---

# 0015: accessibility contrast tokens for text, badges, avatars and the theme checker

- Status: accepted (2026-11-03)
- Issue: #36 (accessibility audit, wave 2) / #20 (keyboard + screen-reader baseline parent)
- Date: 2026-11-03
- Source: implementing the contrast half of `docs/audits/a11y-2026-10.md` — A11Y-01, A11Y-09,
  A11Y-10, A11Y-11, A11Y-12, A11Y-13, A11Y-14 and the avatar half of A11Y-20.

## Context

The October audit measured contrast from the real generated tokens and found the theme checker
(`crates/alloy-theme/src/resolver.rs`) only tested 32 pairs: it never looked at muted text, error
text, link text on subtle/hover, the avatar palette fill, the unread badge, the derived own-bubble
meta, or the translucent material. `themes.json` therefore claimed `passes_aa: true` for all four
shipped themes while A11Y-09/10/11 were visible on screen.

The findings split into three shapes:

1. **One token is used for two jobs.** `color.status.error` was both error *text* (44 `.error`
   rules across forms, dialogs and banners) and a destructive-button *fill* carrying white text.
   Those jobs need opposite luminance: as text on dark surfaces the colour must be light, as a fill
   behind `text.on_accent` it must be dark. No single value passes both in the dark themes.
2. **Derived colours were unchecked.** The own-bubble meta is a `color-mix` in `tokens.css`, and
   the avatar initials colour was a fixed `text-on-accent`. Both are real rendered pairs that the
   token checker could not see.
3. **The material is translucent.** Chrome (86% light / 80% dark panel), the conversation (92% /
   84% canvas) and the floating glass sit over the aurora, so the audit's acceptance for A11Y-14 is
   "use a documented worst-case composited backdrop rather than ignoring the material".

The audit preferred an additive `color.status.error.text` token for the error case. This record
keeps the theme schema unchanged instead, and records the alternative it chose and why.

## Decision

### 1. Error text is fixed by retuning `color.status.error`; the destructive fill is derived

`color.status.error` is now tuned to be legible **as text** on every surface an error can land on
(canvas, subtle, panel, elevated, hover). Because that makes it too light to carry a white label in
the dark themes, the one label-on-fill usage (the destructive `Button`) paints
`--alloy-danger-fill` instead:

```css
/* tokens.css */
--alloy-danger-fill: color-mix(in srgb, var(--alloy-color-status-error) 70%, black);
```

The theme checker mirrors the same 70%+black derivation (`ThemeResolver::danger_fill`) and checks
`text.on_accent` against it (`text_on_accent_danger_fill`). No theme-schema key is added, so old and
third-party themes keep loading; a theme that keeps the old light `status.error` now honestly reports
`error_on_*` failures, which is the point of the checker.

We did **not** add `color.status.error.text` in this wave. The audit preferred it, but every shipped
error style already reads `--alloy-color-status-error` directly (44 rules in `src/lib/components`),
so an additive token would have to be threaded through all of them in the same change or the
finding would stay open in the UI. Retuning the single authoritative token makes the whole app
correct in one place; the dedicated-token migration remains a clean follow-up if a future theme
needs a different dot/icon colour.

### 2. Avatar initials choose their colour from the resolved fill (A11Y-01)

`Avatar.svelte` no longer paints fixed white initials. It resolves the rendered fill with
`getComputedStyle` (`backgroundColor`, falling back to the `--alloy-color-user-N` variable), then
picks between `#ffffff` and `#050505` — whichever has the higher WCAG ratio — via
`pickAvatarOnColor` in `src/lib/identity/userColor.ts`. For the light palettes that is white; for
the dark/AMOLED/midnight pastels it is near-black.

`#ffffff` and `#050505` are chosen as a pair so that one of them **always** clears 4.5:1 for any
possible fill: the worst case is the crossover luminance at ≈4.51:1. The Rust checker reproduces the
same rule (`ThemeResolver::avatar_initials_color`) and asserts the chosen colour against all 12
palette entries in every theme (`avatar_initials_user_1..12`).

### 3. Midnight Ocean muted text, and surface-hover (A11Y-10)

`#64748b` cannot reach 4.5:1 on *any* backdrop (against pure black it measures 4.41:1), so the theme
value had to move, not the surfaces alone:

| midnight-ocean token | before | after |
|---|---|---|
| `text.muted` | `#64748b` | `#8fa1b8` |
| `surface.hover` | `#253256` | `#1f2b4d` |

The darker hover surface gives muted text and error text room on the lightest midnight surface
(muted 5.27:1, error 5.02:1). The theme keeps its slate-blue character; dark's muted was nudged from
`#8491a6` to `#8b98ac` (its elevated-surface pair sat at exactly 4.50:1) and light's from `#65656d`
to `#5c5c64` (margin on the floating-glass worst case, see §7).

### 4. Links are normal-size text: 4.5:1, not 3.0 (A11Y-12)

The resolver's link threshold (`text_link_on_*`) is raised from 3.0 to 4.5 and now covers canvas,
subtle, panel, elevated and hover. 3.0 remains only for genuinely non-text UI (the focus ring,
`accent_primary_on_canvas`). alloy-light's link moves `#2563eb` → `#1d4ed8`.

### 5. The unread badge keeps the mention fill, retuned (A11Y-11)

`--alloy-unread-badge` stays `var(--alloy-color-status-mention)` — one source — and `MobileTabBar`
now paints the badge with that alias. The mention colour is retuned in every theme so
`text.on_accent` on it clears 4.5:1: light `#ea580c` → `#c2410c` (3.56 → 5.18), dark/AMOLED
`#3b82f6` → `#2563eb` (3.68 → 5.17), midnight `#38bdf8` → `#0077b6` (2.14 → 4.87). The checker pair
is `text_on_accent_mention_badge`.

### 6. Own-bubble meta keeps the 82% derivation; midnight's bubble is deepened (A11Y-13)

`--alloy-bubble-own-meta` stays "82% of `bubble.own.text` over `bubble.own.background`" so the
timestamp is still visibly dimmer than the message body. In midnight-ocean that derivation measured
3.79:1 on `#0077b6`, so the own-bubble fill moves to the theme's deeper ocean blue `#075985`
(meta 5.66:1, body text 7.56:1) instead of flattening the meta to full-opacity text (which needed
≥95% and looked identical to the body). The checker computes the same 82% composite
(`bubble_own_meta_on_background`).

### 7. The material's worst case is modeled, not skipped (A11Y-14)

The checker derives two worst-case backdrops per theme and requires primary, secondary, muted and
link text to clear 4.5:1 on each:

```
bloom   = accent.primary at its peak aurora alpha over surface.canvas   (82% light / 42% dark)
chrome  = surface.panel over bloom   (86% light / 80% dark fill)
chat    = surface.canvas over bloom  (92% light / 84% dark fill)
```

Those are the largest excursions the aurora can make from the theme surface, taken from the material
recipe in `tokens.css` / ADR 0014. The floating glass layers have a scrim between them and the
aurora, so they are documented here rather than added to the token checker (the scrim reduces the
composite a lot):

| worst case (light is the limiting theme) | primary | secondary | muted | link | error |
|---|---|---|---|---|---|
| chrome pane (checked) | 14.51 | 6.33 | 5.43 | 5.49 | — |
| conversation pane (checked) | 14.63 | 6.38 | 5.47 | 5.54 | — |
| modal glass: 84% panel over 70% scrim over bloom | 12.64 | 5.51 | 4.72 | 4.78 | 4.61 |
| settings dialog: 84% panel over 55% canvas over bloom | 15.70 | 6.85 | 5.87 | 5.94 | 5.73 |

(The `contrast-a11y.test.ts` guard measures both the checked and the documented rows, so the scrim
formula cannot drift silently.)

### 8. Avatar presence is announced in the name, once (A11Y-20)

The avatar root stays `role="img"`; its accessible name now includes presence
(`"Mai, Status: online"`), the inner `<img>` is decorative (`alt=""`) and the presence dot is
`aria-hidden="true"`. A `role="img"` makes descendants presentational, so a child `aria-label`
could never have been read anyway.

### 9. The checker grows from 32 to 70 pairs per theme

New/replaced pairs, all at 4.5:1 for text and 3.0:1 for non-text:

- muted on canvas/subtle/panel/elevated/hover/bubble-other (6)
- error on canvas/subtle/panel/elevated/hover (5)
- link on canvas/subtle/panel/elevated/hover at 4.5 (5, canvas/threshold replaced)
- `text_on_accent_mention_badge` (1)
- `text_on_accent_danger_fill` (1, derived fill)
- `bubble_own_meta_on_background` (1, derived composite)
- `avatar_initials_user_1..12` (12, derived on-colour)
- primary/secondary/muted/link on chrome worst case (4) and chat worst case (4)

## Token changes

| Theme | Token | Before | After |
|---|---|---|---|
| alloy-light | `text.muted` | `#65656d` | `#5c5c64` |
| alloy-light | `text.link` | `#2563eb` | `#1d4ed8` |
| alloy-light | `status.error` | `#dc2626` | `#b91c1c` |
| alloy-light | `status.mention` | `#ea580c` | `#c2410c` |
| alloy-dark | `text.muted` | `#8491a6` | `#8b98ac` |
| alloy-dark | `status.error` | `#ef4444` | `#f87171` |
| alloy-dark | `status.mention` | `#3b82f6` | `#2563eb` |
| alloy-amoled | — | inherits the dark fixes; its own surfaces already pass | |
| midnight-ocean | `text.muted` | `#64748b` | `#8fa1b8` |
| midnight-ocean | `surface.hover` | `#253256` | `#1f2b4d` |
| midnight-ocean | `status.mention` | `#38bdf8` | `#0077b6` |
| midnight-ocean | `bubble.own.background` | `#0077b6` | `#075985` |

Measured before → after (all ratios ≥4.5:1 after; the app's four shipped themes are
`apps/alloy/ui/src/lib/theme/generated/themes.css`):

| Theme | Pair | Before | After |
|---|---|---|---|
| alloy-light | error on canvas / subtle / hover | 4.38 / 4.14 / 3.95 | 5.87 / 5.55 / 5.30 |
| alloy-light | link on subtle / hover | 4.44 / 4.23 | 5.75 / 5.49 |
| alloy-light | white on mention badge | 3.56 | 5.18 |
| alloy-light | destructive label on `status.error` | 4.83 | 10.25 (derived fill `#821414`) |
| alloy-dark | error on subtle / panel / elevated / hover | 4.38 / 4.38 / 3.81 / 3.97 | 5.96 / 5.96 / 5.19 / 5.41 |
| alloy-dark | muted on elevated / bubble-other | 4.50 / 4.50 | 4.91 / 4.91 |
| alloy-dark | white on mention badge | 3.68 | 5.17 |
| alloy-dark | destructive label on `status.error` | 3.77 | 5.22 (derived fill `#ae4f4f`) |
| alloy-amoled | error on elevated | 4.80 | 6.53 |
| alloy-amoled | white on mention badge | 3.68 | 5.17 |
| midnight-ocean | muted on canvas / panel / subtle / elevated / hover / bubble-other | 3.86 / 3.53 / 3.17 / 3.17 / 2.64 / 3.17 | 6.97 / 6.36 / 5.72 / 5.72 / 5.27 / 5.72 |
| midnight-ocean | white on mention badge | 2.14 | 4.87 |
| midnight-ocean | own-bubble meta (82% composite) | 3.79 | 5.66 |
| midnight-ocean | destructive label on `status.error` | 2.76 | 5.22 |
| dark/amoled/midnight | white initials on user palettes | 1.67–2.98 | 6.83–12.21 (chosen `#050505`) |
| alloy-light | white initials on user palettes | 5.02–7.90 | unchanged (white still wins) |

## Alternatives considered

- **Add `color.status.error.text` and migrate every `.error` call site.** The audit's preference,
  but it is a 44-call-site component sweep in the same change or the UI finding stays open. Retuning
  the one token the components already read fixes the whole app now, with no schema change and no
  backwards-compatibility risk. A future theme that needs a separate dot colour can still add the
  token then.
- **Use `accent.fill` for the unread badge.** It passes, but it would drop the mention/notification
  colour from the badge, and the audit's acceptance asked for the badge fill itself to be checked.
  Retuning mention (also used for the orange tag and the aurora tint) keeps the semantic.
- **Lift the own-bubble meta to 95%+ opacity in `tokens.css`.** It only just passes in midnight
  (4.58) and the timestamp then looks identical to the body. Deepening midnight's own bubble keeps
  the 82% hierarchy and clears with margin.
- **Choose the avatar on-colour from a luminance threshold** (e.g. "fill lighter than 50% → black").
  Comparing the two real ratios is simpler to mirror exactly in Rust and is provably ≥4.51:1 for any
  fill, including theme palettes the audit never saw.
- **Ignore translucent material in the checker** (the audit's A11Y-14 explicitly rejects this).
  The composited worst case above is checked; the scrimmed floating glass is documented and
  measured in the UI guard.

## Consequences

- `themes.css` and `themes.json` are regenerated: `passes_aa: true` for all four themes and
  `checks_count` 32 → 70.
- Third-party themes that keep the old values now fail the checker, which is correct: those were
  real AA failures. Theme authors get the failing pair names and ratios.
- `--alloy-danger-fill` (70% error + black) and `--alloy-bubble-own-meta` (82%) are UI-side
  derivations mirrored in the Rust checker and in `contrast-a11y.test.ts`; they are commented in
  both places and guarded, so changing one without the other fails a test.
- Visual deltas: light error/destructive/link are darker; midnight muted is lighter, hover is
  darker, mention is a deeper blue and the own bubble is deep ocean; dark/AMOLED error is a lighter
  red, muted a touch lighter and mention a darker blue. No layout, radius or typography changes, and
  the "accent fill for own messages / mention accent for badges" language of ADR 0001/0014 stays.
- Two other `status.error` fills outside the named A11Y-09 sites carried an on-accent label:
  `LiveLocationBanner.svelte` `.stop` and `Timeline.svelte` `.toast`. Both now use
  `--alloy-danger-fill` too (the `Timeline.svelte` line was applied by the conversation-wave owner
  to avoid concurrent edits to that file). Both were already under 4.5:1 in dark before this change
  (3.77:1) and would be 2.76:1 with the text-tuned `status.error`, so the derived fill is required
  there as well. `contrast-a11y.test.ts` pins all three filled controls to the token.
- Follow-up (not in this change set): the timeline jump-to-latest badge still paints on-accent on
  `accent.primary`, which is 3.68:1 in alloy-dark/AMOLED (light and midnight pass). It is outside the
  audited A11Y-11 (phone tab bar) and needs the same treatment — use `--alloy-unread-badge` or
  `accent.fill`. Also worth a later `prefers-contrast` variant if the material changes.

## References

- `docs/audits/a11y-2026-10.md` — findings A11Y-01/09/10/11/12/13/14/20
- ADR 0001 — UI visual language v1 (sender colours, no colour-only indicators)
- ADR 0014 — flush, lightly transparent shell (material recipe and worst-case contract)
- `crates/alloy-theme/src/resolver.rs` — checker and derivations
- `packages/themes/*.json`, `apps/alloy/ui/src/lib/theme/generated/themes.{css,json}`
- `apps/alloy/ui/src/lib/tokens.css`, `src/lib/identity/userColor.ts`,
  `src/lib/components/common/Avatar.svelte`, `common/Button.svelte`,
  `shell/MobileTabBar.svelte`
- `apps/alloy/ui/src/lib/theme/contrast-a11y.test.ts` — independent measurement of the generated CSS
