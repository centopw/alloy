<script lang="ts">
  import { portal } from '$lib/platform/portal';
  import {
    accessibleName,
    INTERACTIVE_SELECTOR,
    placeTooltip,
    stashTitle,
    TOOLTIP_ID,
    tooltipShortcut,
    tooltipText,
    type AnchorRect,
  } from '$lib/platform/tooltip';

  const HOVER_DELAY_MS = 450;
  const FOCUS_DELAY_MS = 200;
  /** A tooltip shown within this window makes the next hover appear immediately. */
  const WARM_WINDOW_MS = 300;

  interface Shown {
    text: string;
    shortcut: string | null;
    anchor: HTMLElement;
    rect: AnchorRect;
    /** True when aria-describedby was added because the tooltip adds information to the name. */
    described: boolean;
    prevDescribedBy: string | null;
  }

  let shown = $state.raw<Shown | null>(null);
  let timer: number | null = null;
  let lastHiddenAt = 0;
  /** Set while the document listeners are being torn down: events they still see must not write state. */
  let disposed = false;

  function interactiveFrom(target: EventTarget | null): HTMLElement | null {
    if (!(target instanceof Element)) return null;
    const el = target.closest(INTERACTIVE_SELECTOR);
    return el instanceof HTMLElement ? el : null;
  }

  function clearTimer() {
    if (timer !== null) {
      window.clearTimeout(timer);
      timer = null;
    }
  }

  /** Undo the aria-describedby link added for the currently shown tooltip. */
  function undescribe(s: Shown) {
    if (!s.described) return;
    if (s.prevDescribedBy === null) s.anchor.removeAttribute('aria-describedby');
    else s.anchor.setAttribute('aria-describedby', s.prevDescribedBy);
  }

  function hide() {
    clearTimer();
    // Unmounting this layer blurs whatever had focus, and that focusout reaches this handler while
    // the effect is being destroyed. Writing `shown` there is a state mutation during teardown,
    // which Svelte rejects; the teardown itself already un-describes the anchor.
    if (disposed) return;
    if (shown === null) return;
    undescribe(shown);
    lastHiddenAt = Date.now();
    shown = null;
  }

  /**
   * Event entry point for hiding. A dialog that removes its DOM blurs the control inside it, and
   * that `focusout` arrives while Svelte is still tearing the dialog's block effect down — writing
   * `shown` synchronously there is the `state_unsafe_mutation` the walk recorded. A microtask runs
   * after that flush, where the write is ordinary state.
   */
  function hideSoon() {
    queueMicrotask(hide);
  }

  function show(el: HTMLElement) {
    clearTimer();
    stashTitle(el);
    const text = tooltipText(el);
    if (text === null) {
      hide();
      return;
    }
    if (shown !== null) undescribe(shown);
    const described = accessibleName(el) !== text;
    const prev = el.getAttribute('aria-describedby');
    if (described) {
      el.setAttribute('aria-describedby', prev ? `${prev} ${TOOLTIP_ID}` : TOOLTIP_ID);
    }
    shown = {
      text,
      shortcut: tooltipShortcut(el),
      anchor: el,
      rect: el.getBoundingClientRect(),
      described,
      prevDescribedBy: prev,
    };
  }

  function schedule(el: HTMLElement, delay: number) {
    clearTimer();
    timer = window.setTimeout(() => {
      timer = null;
      show(el);
    }, delay);
  }

  function onPointerOver(e: PointerEvent) {
    if (e.pointerType === 'touch') return;
    const el = interactiveFrom(e.target);
    if (el === null || el === shown?.anchor) return;
    const warm = Date.now() - lastHiddenAt < WARM_WINDOW_MS;
    schedule(el, warm ? 0 : HOVER_DELAY_MS);
  }

  function onPointerOut(e: PointerEvent) {
    const from = interactiveFrom(e.target);
    if (from === null) return;
    if (e.relatedTarget instanceof Node && from.contains(e.relatedTarget)) return;
    hideSoon();
  }

  function onFocusIn(e: FocusEvent) {
    const el = interactiveFrom(e.target);
    if (el === null || el === shown?.anchor) return;
    if (!el.matches(':focus-visible')) return;
    schedule(el, FOCUS_DELAY_MS);
  }

  function onFocusOut(e: FocusEvent) {
    if (interactiveFrom(e.target) !== null) hideSoon();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') hideSoon();
  }

  // Document-level capture listeners: one set for the whole app, active for the layer's lifetime.
  $effect(() => {
    disposed = false;
    document.addEventListener('pointerover', onPointerOver, true);
    document.addEventListener('pointerout', onPointerOut, true);
    document.addEventListener('pointerdown', hideSoon, true);
    document.addEventListener('focusin', onFocusIn, true);
    document.addEventListener('focusout', onFocusOut, true);
    document.addEventListener('keydown', onKeyDown, true);
    document.addEventListener('scroll', hideSoon, true);
    window.addEventListener('resize', hideSoon);
    return () => {
      // Before the listeners come off: a blur caused by this teardown can still dispatch focusout
      // synchronously, and `hide()` must be inert by then rather than write state mid-teardown.
      disposed = true;
      clearTimer();
      document.removeEventListener('pointerover', onPointerOver, true);
      document.removeEventListener('pointerout', onPointerOut, true);
      document.removeEventListener('pointerdown', hideSoon, true);
      document.removeEventListener('focusin', onFocusIn, true);
      document.removeEventListener('focusout', onFocusOut, true);
      document.removeEventListener('keydown', onKeyDown, true);
      document.removeEventListener('scroll', hideSoon, true);
      window.removeEventListener('resize', hideSoon);
      if (shown !== null) undescribe(shown);
    };
  });

  /** Position the bubble above its anchor, measuring the rendered size. */
  function place(node: HTMLElement, s: Shown) {
    const apply = (cur: Shown) => {
      const p = placeTooltip(
        cur.rect,
        { width: node.offsetWidth, height: node.offsetHeight },
        { width: window.innerWidth, height: window.innerHeight },
      );
      node.style.left = `${p.x}px`;
      node.style.top = `${p.y}px`;
    };
    apply(s);
    return { update: apply };
  }
</script>

{#if shown}
  <div
    id={TOOLTIP_ID}
    class="tooltip"
    role="tooltip"
    data-alloy-tooltip=""
    use:portal
    use:place={shown}
  >
    <span class="text">{shown.text}</span>
    {#if shown.shortcut}
      <kbd class="shortcut">{shown.shortcut}</kbd>
    {/if}
  </div>
{/if}

<style>
  .tooltip {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 120;
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    max-width: 280px;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-text-primary);
    color: var(--alloy-color-surface-canvas);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-medium);
    line-height: var(--alloy-typography-line-height-tight);
    pointer-events: none;
    animation: tooltip-in var(--alloy-motion-duration-fast) ease-out;
  }

  .shortcut {
    padding: 0 var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-sm);
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-xs);
    color: inherit;
  }

  @keyframes tooltip-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .tooltip {
      animation: none;
    }
  }
</style>
