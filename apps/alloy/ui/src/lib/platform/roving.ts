/**
 * Roving-tabindex movement for composite widgets (tablists, radiogroups, toolbars, menus).
 *
 * A composite widget has one tab stop, not one per child, and the arrow keys move the active
 * child (WAI-ARIA Authoring Practices). The audit found tablists and a radiogroup that declared
 * the role without the keyboard pattern (A11Y-17); this pure helper is shared by each fix and is
 * unit-tested without a DOM.
 */

export type RovingOrientation = 'horizontal' | 'vertical' | 'both';

export interface RovingOptions {
  /** Which arrow keys participate. A tablist is horizontal; a radiogroup is usually vertical. */
  orientation: RovingOrientation;
  /** Right-to-left layouts mirror the horizontal arrows. */
  rtl?: boolean;
  /** Wrap from the last item to the first. Defaults to true (the ARIA tab/radio pattern). */
  loop?: boolean;
}

/**
 * The index a key press moves the active child to, or `null` when the key is not one this widget
 * handles (so the caller can leave it to the page).
 *
 * Home/End always work. Arrow keys clamp at the ends unless `loop` is set, in which case they
 * wrap. Vertical lists ignore Left/Right and vice versa.
 */
export function nextRovingIndex(
  current: number,
  count: number,
  key: string,
  options: RovingOptions
): number | null {
  if (count <= 0) return null;
  const loop = options.loop ?? true;
  const rtl = options.rtl ?? false;
  const start = current >= 0 && current < count ? current : 0;

  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;

  const horizontal = options.orientation === 'horizontal' || options.orientation === 'both';
  const vertical = options.orientation === 'vertical' || options.orientation === 'both';

  let delta = 0;
  if (horizontal && key === 'ArrowRight') delta = rtl ? -1 : 1;
  else if (horizontal && key === 'ArrowLeft') delta = rtl ? 1 : -1;
  else if (vertical && key === 'ArrowDown') delta = 1;
  else if (vertical && key === 'ArrowUp') delta = -1;
  else return null;

  const next = start + delta;
  if (next < 0) return loop ? count - 1 : 0;
  if (next >= count) return loop ? 0 : count - 1;
  return next;
}

/**
 * Builds the `tabindex` a child should carry: 0 for the active child, -1 for the rest. A widget
 * with no active child keeps the first one tabbable so it can be entered with Tab at all.
 */
export function rovingTabindex(index: number, active: number): 0 | -1 {
  if (active < 0) return index === 0 ? 0 : -1;
  return index === active ? 0 : -1;
}
