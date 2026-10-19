/**
 * Scroll offset that keeps the same distance between the visible bottom edge and the end of the
 * content after the viewport changed height. When the on-screen keyboard opens the viewport shrinks
 * from below: holding this gap keeps whatever the user was looking at in view (at the bottom it stays
 * pinned to the newest message, scrolled up it stays where it was) instead of revealing older rows.
 */
export function offsetForBottomGap(scrollSize: number, viewportSize: number, bottomGap: number): number {
  return Math.max(0, scrollSize - viewportSize - Math.max(0, bottomGap));
}
