/** How far (px) a list must be pulled down from its top before letting go refreshes. */
export const PULL_THRESHOLD_PX = 80;

/** Whether a finished pull of `dy` px started at the top of the list should refresh. */
export function shouldRefresh(dy: number, startedAtTop: boolean): boolean {
  return startedAtTop && dy >= PULL_THRESHOLD_PX;
}

interface Options {
  onrefresh: () => void;
  enabled?: boolean;
}

/**
 * Svelte action for phones: pulling a scrolled-to-top list down by `PULL_THRESHOLD_PX` and letting go runs
 * `onrefresh`. Sideways drags and pulls that start below the top of the list are ignored, and the list keeps
 * scrolling as usual (nothing is prevented).
 */
export function pullToRefresh(node: HTMLElement, options: Options) {
  let current = options;
  let startY = 0;
  let startX = 0;
  let tracking = false;
  let atTop = false;
  let dy = 0;
  let sideways = false;

  function onStart(e: TouchEvent) {
    if (current.enabled === false || e.touches.length !== 1) return;
    tracking = true;
    atTop = node.scrollTop <= 0;
    sideways = false;
    dy = 0;
    startY = e.touches[0].clientY;
    startX = e.touches[0].clientX;
  }

  function onMove(e: TouchEvent) {
    if (!tracking || e.touches.length !== 1) return;
    const touch = e.touches[0];
    dy = touch.clientY - startY;
    if (Math.abs(touch.clientX - startX) > Math.abs(dy)) sideways = true;
    if (node.scrollTop > 0) atTop = false;
  }

  function onEnd() {
    if (!tracking) return;
    tracking = false;
    if (!sideways && shouldRefresh(dy, atTop)) current.onrefresh();
  }

  node.addEventListener('touchstart', onStart, { passive: true });
  node.addEventListener('touchmove', onMove, { passive: true });
  node.addEventListener('touchend', onEnd);
  node.addEventListener('touchcancel', () => (tracking = false));
  return {
    update(next: Options) {
      current = next;
    },
    destroy() {
      node.removeEventListener('touchstart', onStart);
      node.removeEventListener('touchmove', onMove);
      node.removeEventListener('touchend', onEnd);
    },
  };
}
