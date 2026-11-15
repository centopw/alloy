/**
 * Pure reordering for the pinned block of the room list: the sidebar's drag-and-drop and
 * Alt+Arrow handlers both reduce to "move this id to that index" and hand the result to
 * `roomsStore.reorderPinned`.
 */

/**
 * Moves `fromId` so that it ends up at `toIndex` in the returned list. `toIndex` is clamped to the
 * list; an unknown `fromId` or a move that changes nothing returns the input order unchanged (a new
 * array either way, so callers can compare by content).
 */
export function movePinned(ids: readonly string[], fromId: string, toIndex: number): string[] {
  const from = ids.indexOf(fromId);
  if (from === -1) return [...ids];
  const to = Math.max(0, Math.min(ids.length - 1, Math.trunc(toIndex)));
  if (to === from) return [...ids];
  const next = [...ids];
  next.splice(from, 1);
  next.splice(to, 0, fromId);
  return next;
}

/** True when the two orders differ, so a no-op drop never reaches the engine. */
export function orderChanged(before: readonly string[], after: readonly string[]): boolean {
  return before.length !== after.length || before.some((id, index) => id !== after[index]);
}
