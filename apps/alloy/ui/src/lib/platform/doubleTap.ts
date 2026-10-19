/** Two touch taps within this window count as a double tap. */
export const DOUBLE_TAP_MS = 300;
/** ...and must land within this distance of each other. */
export const DOUBLE_TAP_SLOP_PX = 24;
/** A touch that travels further than this is a drag, not a tap. */
const TAP_SLOP_PX = 10;
/** Taps on these keep their own meaning. */
const INTERACTIVE = 'button, a[href], input, textarea, select, video, audio, .reactions';

export interface TapPoint {
  x: number;
  y: number;
  at: number;
}

/** Whether `next` completes a double tap that started with `prev`. */
export function isDoubleTap(prev: TapPoint | null, next: TapPoint): boolean {
  if (!prev) return false;
  return next.at - prev.at < DOUBLE_TAP_MS && Math.hypot(next.x - prev.x, next.y - prev.y) <= DOUBLE_TAP_SLOP_PX;
}

/**
 * Svelte action: calls `handler(point)` on a touch double tap. Mouse and pen are ignored, so desktop
 * double-click keeps selecting text. Taps on buttons, links and media controls are ignored.
 */
export function doubleTap(node: HTMLElement, initial: (point: { x: number; y: number }) => void) {
  let handler = initial;
  let last: TapPoint | null = null;
  let down: { x: number; y: number } | null = null;

  const onDown = (e: PointerEvent) => {
    down = e.pointerType === 'touch' ? { x: e.clientX, y: e.clientY } : null;
  };

  const onUp = (e: PointerEvent) => {
    if (e.pointerType !== 'touch' || !down) return;
    const moved = Math.hypot(e.clientX - down.x, e.clientY - down.y);
    down = null;
    if (moved > TAP_SLOP_PX) {
      last = null;
      return;
    }
    if (e.target instanceof Element && e.target.closest(INTERACTIVE)) {
      last = null;
      return;
    }
    const tap = { x: e.clientX, y: e.clientY, at: e.timeStamp };
    if (isDoubleTap(last, tap)) {
      last = null;
      e.preventDefault();
      handler({ x: tap.x, y: tap.y });
    } else {
      last = tap;
    }
  };

  node.addEventListener('pointerdown', onDown);
  node.addEventListener('pointerup', onUp);
  return {
    update(next: (point: { x: number; y: number }) => void) {
      handler = next;
    },
    destroy() {
      node.removeEventListener('pointerdown', onDown);
      node.removeEventListener('pointerup', onUp);
    },
  };
}
