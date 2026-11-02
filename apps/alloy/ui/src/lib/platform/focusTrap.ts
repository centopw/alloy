/**
 * Tab-focus containment for surfaces that declare `aria-modal="true"`.
 *
 * A surface that tells assistive technology the rest of the page is inert must keep the keyboard
 * inside itself, otherwise `aria-modal` is a lie (A11Y-16). The decision of *what* to focus is
 * pure and unit-tested here; the components only supply their focusables and focused element.
 */

/** Elements a browser makes tabbable. Disabled controls and `tabindex="-1"` are excluded. */
export const FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Returns the element focus must move to for a Tab press that would leave the surface, or `null`
 * when the browser should handle the press normally.
 *
 * - Shift+Tab on the first focusable wraps to the last one.
 * - Tab on the last focusable wraps to the first one.
 * - If focus somehow sits outside the surface (for example on `<body>` after a re-render), it is
 *   pulled back in: the last element for Shift+Tab, the first otherwise.
 */
export function trapTabTarget<T extends Element>(
  focusables: readonly T[],
  active: Element | null,
  shift: boolean
): T | null {
  if (focusables.length === 0) return null;
  const first = focusables[0];
  const last = focusables[focusables.length - 1];
  const index = active === null ? -1 : focusables.indexOf(active as T);
  if (index < 0) return shift ? last : first;
  if (shift && index === 0) return last;
  if (!shift && index === focusables.length - 1) return first;
  return null;
}

/**
 * DOM helper for the components: the tabbable elements inside `root` that are actually laid out.
 * Elements hidden with `display: none` (the desktop-only back button on phones, for example) are
 * dropped so a wrap never sends focus to an element the user cannot see.
 */
export function visibleFocusables(root: ParentNode | null | undefined): HTMLElement[] {
  if (!root) return [];
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (el) => el.getClientRects().length > 0
  );
}

/**
 * The single Tab handler every modal surface can reuse. Returns `true` when it moved focus (the
 * caller must then call `preventDefault()`), `false` to let the browser do its normal thing.
 */
export function handleModalTabKey(
  event: Pick<KeyboardEvent, 'key' | 'shiftKey'>,
  root: ParentNode | null | undefined,
  active: Element | null
): boolean {
  if (event.key !== 'Tab') return false;
  const target = trapTabTarget(visibleFocusables(root), active, event.shiftKey);
  if (!target) return false;
  target.focus();
  return true;
}
