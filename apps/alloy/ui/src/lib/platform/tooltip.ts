/**
 * Pure helpers behind the global tooltip layer (components/common/TooltipLayer.svelte).
 * Kept free of DOM listeners so the decisions are unit-testable.
 */

export const TOOLTIP_ID = 'alloy-tooltip';
export const TOOLTIP_ROOT_ATTR = 'data-alloy-tooltip';
/** Marks an aria-label that stashTitle() mirrored from a removed native title attribute. */
export const TITLE_MIRROR_ATTR = 'data-alloy-title-label';

export const INTERACTIVE_SELECTOR = [
  'button',
  'a[href]',
  '[role="button"]',
  '[role="tab"]',
  '[role="menuitem"]',
  '[role="switch"]',
  '[role="checkbox"]',
  '[role="radio"]',
  '[role="link"]',
  'input[type="checkbox"]',
  'input[type="radio"]',
  'input[type="range"]',
].join(',');

export interface AnchorRect {
  left: number;
  top: number;
  bottom: number;
  width: number;
}

export interface BoxSize {
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

export function isInteractive(el: Element): boolean {
  return el.matches(INTERACTIVE_SELECTOR);
}

/** Collapse whitespace; null when nothing is left. */
export function normalizeText(value: string | null | undefined): string | null {
  const text = (value ?? '').replace(/\s+/g, ' ').trim();
  return text.length > 0 ? text : null;
}

/**
 * Text for the tooltip bubble, or null when no tooltip should show. Precedence:
 * data-tooltip > aria-label > title > data-title (the title stashed by stashTitle).
 * Null for non-interactive elements, elements inside the tooltip itself, and when the text
 * merely repeats the visible label (a tooltip would add nothing).
 */
export function tooltipText(el: HTMLElement): string | null {
  if (!isInteractive(el)) return null;
  if (el.closest(`[${TOOLTIP_ROOT_ATTR}]`)) return null;
  const text =
    normalizeText(el.dataset.tooltip) ??
    normalizeText(el.getAttribute('aria-label')) ??
    normalizeText(el.getAttribute('title')) ??
    normalizeText(el.dataset.title);
  if (text === null) return null;
  if (text === normalizeText(el.textContent)) return null;
  return text;
}

/** Keyboard shortcut hint from data-shortcut, shown as a chip beside the tooltip text. */
export function tooltipShortcut(el: HTMLElement): string | null {
  return normalizeText(el.dataset.shortcut);
}

/**
 * Accessible name in the same order browsers use: labelledby, aria-label, labels, content, title.
 * Deliberately ignores data-title: that attribute holds a title already moved off the element,
 * so it does not count as a name the element still has.
 */
export function accessibleName(el: HTMLElement): string | null {
  const labelledBy = el.getAttribute('aria-labelledby');
  if (labelledBy) {
    const joined = labelledBy
      .split(/\s+/)
      .map((id) => el.ownerDocument.getElementById(id)?.textContent ?? '')
      .join(' ');
    const fromLabelledBy = normalizeText(joined);
    if (fromLabelledBy !== null) return fromLabelledBy;
  }
  const ariaLabel = normalizeText(el.getAttribute('aria-label'));
  if (ariaLabel !== null) return ariaLabel;
  if (el instanceof HTMLInputElement && el.labels) {
    const fromLabels = normalizeText(
      Array.from(el.labels)
        .map((label) => label.textContent ?? '')
        .join(' '),
    );
    if (fromLabels !== null) return fromLabels;
  }
  const content = normalizeText(el.textContent);
  if (content !== null) return content;
  return normalizeText(el.getAttribute('title'));
}

/**
 * Move a native title attribute into data-title so the browser's own delayed tooltip does not
 * double up with the layer. When the element would lose its accessible name, the title is
 * mirrored into aria-label (marked with TITLE_MIRROR_ATTR).
 */
export function stashTitle(el: HTMLElement): void {
  const title = el.getAttribute('title');
  if (title === null) return;
  el.removeAttribute('title');
  const text = normalizeText(title);
  if (text === null) return;
  if (accessibleName(el) === null) {
    el.setAttribute('aria-label', text);
    el.setAttribute(TITLE_MIRROR_ATTR, '');
  }
  el.dataset.title = text;
}

/**
 * Place a bubble of `size` above `anchor`, centred on it. Falls back below the anchor when there
 * is no room above, then clamps horizontally and vertically inside the viewport with `margin`.
 */
export function placeTooltip(
  anchor: AnchorRect,
  size: BoxSize,
  viewport: BoxSize,
  gap = 6,
  margin = 8,
): Point {
  let y = anchor.top - gap - size.height;
  if (y < margin) y = anchor.bottom + gap;
  const centred = anchor.left + anchor.width / 2 - size.width / 2;
  const x = Math.max(margin, Math.min(centred, viewport.width - size.width - margin));
  y = Math.max(margin, Math.min(y, viewport.height - size.height - margin));
  return { x, y };
}
