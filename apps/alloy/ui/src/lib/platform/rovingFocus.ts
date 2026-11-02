/**
 * Roving-tabindex model for composite widgets: the whole list is one Tab stop, and arrow keys move
 * the focused row. Kept pure so the keyboard contract can be unit-tested without a component
 * renderer (none is installed). Used by the desktop room list.
 */

export const ROVING_KEYS = ['ArrowDown', 'ArrowUp', 'Home', 'End', 'PageDown', 'PageUp'] as const;
export type RovingKey = (typeof ROVING_KEYS)[number];

/** Whether `key` is handled by the roving model. Anything else (Enter, Space, letters) is left alone. */
export function isRovingKey(key: string): key is RovingKey {
  return (ROVING_KEYS as readonly string[]).includes(key);
}

export interface RovingOptions {
  /** Rows moved by PageUp/PageDown; defaults to 10. */
  pageSize?: number;
}

/**
 * The row index that should take focus after `key` is pressed in a list of `count` rows whose
 * focused row is `current`. Returns null for keys the model does not handle.
 *
 * Arrow keys wrap at the ends of the list; Home/End jump to the first/last row; PageUp/PageDown
 * move by `pageSize` and clamp. A `current` outside the list (-1, or stale after a filter change)
 * moves to the first row.
 */
export function rovingTargetIndex(
  count: number,
  current: number,
  key: string,
  options: RovingOptions = {}
): number | null {
  if (count <= 0 || !isRovingKey(key)) return null;
  const last = count - 1;
  const from = current >= 0 && current < count ? current : 0;
  const page = Math.max(1, Math.floor(options.pageSize ?? 10));
  switch (key) {
    case 'ArrowDown':
      return from === last ? 0 : from + 1;
    case 'ArrowUp':
      return from === 0 ? last : from - 1;
    case 'Home':
      return 0;
    case 'End':
      return last;
    case 'PageDown':
      return Math.min(from + page, last);
    case 'PageUp':
      return Math.max(from - page, 0);
  }
}

/**
 * The single row that owns the list's Tab stop: the row focused last, else the open room, else the
 * first rendered row. Null only for an empty list.
 */
export function rovingTabStop(
  ids: readonly string[],
  lastFocused: string | null,
  active: string | null = null
): string | null {
  if (lastFocused !== null && ids.includes(lastFocused)) return lastFocused;
  if (active !== null && ids.includes(active)) return active;
  return ids[0] ?? null;
}
