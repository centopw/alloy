/**
 * The paging model behind the attachment preview. Kept out of the component so the arithmetic the
 * user depends on — which file is showing after a removal, and where the arrows go — is testable.
 */

/** The index to show for a list of `count` files, never past an end and never negative. */
export function clampIndex(index: number, count: number): number {
  if (count <= 0) return 0;
  if (!Number.isFinite(index) || index < 0) return 0;
  return Math.min(Math.trunc(index), count - 1);
}

/**
 * The index after stepping `delta` files through a list of `count`. The ends stop rather than wrap,
 * so a person paging with the keyboard can feel where the album begins and ends.
 */
export function moveIndex(index: number, count: number, delta: number): number {
  return clampIndex(clampIndex(index, count) + delta, count);
}

/**
 * The file to show after the visible one was removed. Staying on the same index keeps the eye on the
 * next file, which is what a person deleting from the end of an album expects.
 */
export function indexAfterRemoval(index: number, remaining: number): number {
  return clampIndex(index, remaining);
}
