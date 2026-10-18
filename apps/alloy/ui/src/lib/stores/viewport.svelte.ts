/**
 * Device class shared by every component. `isPhone` selects the mobile UI (one screen at a time,
 * bottom tab bar, full-screen pages, bottom sheets); it is the same breakpoint as the shell's
 * `narrow` layout (below 900px). `isTouch` is true for coarse pointers (phones, tablets).
 */
export const PHONE_MAX_WIDTH = 899;

class ViewportStore {
  width = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);
  isTouch = $state(
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(pointer: coarse)').matches
      : false
  );
  readonly isPhone = $derived(this.width <= PHONE_MAX_WIDTH);

  constructor() {
    if (typeof window === 'undefined') return;
    window.addEventListener('resize', () => (this.width = window.innerWidth));
    if (typeof window.matchMedia === 'function') {
      const mq = window.matchMedia('(pointer: coarse)');
      mq.addEventListener?.('change', (e) => (this.isTouch = e.matches));
    }
  }
}

export const viewport = new ViewportStore();
