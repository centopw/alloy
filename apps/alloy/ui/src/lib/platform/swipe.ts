/**
 * Horizontal touch swipe on an element: swipe right on a message to reply, swipe left on a chat row for
 * its actions. The gesture only claims the touch when it is clearly horizontal, so vertical scrolling
 * and the long-press menu keep working.
 */
import { contextMenu } from '$lib/stores/contextmenu.svelte';

/** Movement before the gesture is classified as a swipe or as something else. */
const DECIDE_PX = 10;
/** How far past the threshold the content may still be dragged, as a fraction of the threshold. */
const OVERSHOOT = 0.5;
/** Drag resistance beyond the threshold. */
const RESISTANCE = 0.4;

export type SwipeDirection = 'left' | 'right';
export type SwipeIntent = 'undecided' | 'swipe' | 'reject';

/** Whether a touch moved by (`dx`, `dy`) is a swipe toward `direction`, still unclear, or not a swipe. */
export function classifySwipe(dx: number, dy: number, direction: SwipeDirection): SwipeIntent {
  const ax = Math.abs(dx);
  const ay = Math.abs(dy);
  if (Math.max(ax, ay) < DECIDE_PX) return 'undecided';
  if (ax <= 2 * ay) return 'reject';
  return (direction === 'right' ? dx > 0 : dx < 0) ? 'swipe' : 'reject';
}

/** Distance dragged toward `direction`, never negative. */
export function swipeTravel(dx: number, direction: SwipeDirection): number {
  return Math.max(0, direction === 'right' ? dx : -dx);
}

/** On-screen offset for `travel`: 1:1 up to `threshold`, then resisted and capped. */
export function swipeOffset(travel: number, threshold: number): number {
  if (travel <= threshold) return travel;
  return Math.min(threshold + (travel - threshold) * RESISTANCE, threshold * (1 + OVERSHOOT));
}

export interface SwipeOptions {
  direction: SwipeDirection;
  /** Travel in px up to which the content follows the finger 1:1; beyond it the drag resists. */
  threshold: number;
  /** Travel in px that commits the swipe on release. Defaults to `threshold`. */
  commitAt?: number;
  enabled?: boolean;
  /** Touches starting this close to a screen edge are left to the system back gesture. */
  edgeGuard?: number;
  /** Follows the finger: the on-screen offset in px (always >= 0, toward `direction`). */
  onmove?: (offset: number, committable: boolean) => void;
  /** The gesture finished or was cancelled; the content should spring back. */
  onend?: () => void;
  /** Released beyond the threshold. `travel` is the raw distance dragged. */
  oncommit: (travel: number) => void;
}

export function swipe(node: HTMLElement, initial: SwipeOptions) {
  let opts = initial;
  let pointerId: number | null = null;
  let startX = 0;
  let startY = 0;
  let travel = 0;
  let claimed = false;
  let crossed = false;

  const reset = () => {
    const wasClaimed = claimed;
    pointerId = null;
    claimed = false;
    crossed = false;
    travel = 0;
    if (wasClaimed) opts.onend?.();
  };

  const onDown = (e: PointerEvent) => {
    if (e.pointerType !== 'touch' || !opts.enabled || pointerId !== null) return;
    const guard = opts.edgeGuard ?? 0;
    if (guard > 0 && (e.clientX < guard || e.clientX > window.innerWidth - guard)) return;
    pointerId = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
  };

  const onMove = (e: PointerEvent) => {
    if (e.pointerId !== pointerId) return;
    if (contextMenu.isOpen) {
      reset();
      return;
    }
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (!claimed) {
      const intent = classifySwipe(dx, dy, opts.direction);
      if (intent === 'reject') {
        pointerId = null;
        return;
      }
      if (intent === 'undecided') return;
      claimed = true;
      try {
        node.setPointerCapture(e.pointerId);
      } catch {
        /* the pointer is already gone */
      }
    }
    travel = swipeTravel(dx, opts.direction);
    const committable = travel >= (opts.commitAt ?? opts.threshold);
    if (committable && !crossed) navigator.vibrate?.(10);
    crossed = committable;
    opts.onmove?.(swipeOffset(travel, opts.threshold), committable);
  };

  const onUp = (e: PointerEvent) => {
    if (e.pointerId !== pointerId) return;
    const commit = claimed && travel >= (opts.commitAt ?? opts.threshold);
    const dragged = travel;
    reset();
    if (commit) opts.oncommit(dragged);
  };

  const onCancel = (e: PointerEvent) => {
    if (e.pointerId === pointerId) reset();
  };

  // A claimed swipe must not end in a click on whatever lies under the finger.
  const onClick = (e: MouseEvent) => {
    if (claimed) e.stopPropagation();
  };

  node.style.touchAction = 'pan-y';
  node.addEventListener('pointerdown', onDown);
  node.addEventListener('pointermove', onMove);
  node.addEventListener('pointerup', onUp);
  node.addEventListener('pointercancel', onCancel);
  node.addEventListener('click', onClick, true);
  return {
    update(next: SwipeOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener('pointerdown', onDown);
      node.removeEventListener('pointermove', onMove);
      node.removeEventListener('pointerup', onUp);
      node.removeEventListener('pointercancel', onCancel);
      node.removeEventListener('click', onClick, true);
    },
  };
}
