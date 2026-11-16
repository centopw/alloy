<script lang="ts">
  /**
   * A picture at full size, over the conversation.
   *
   * Clicking a picture used to hand its file path to the operating system, which opened whatever
   * the OS associates with it — or nothing, on a build where the opener refuses local paths. The
   * lightbox keeps the picture inside the app: one surface, one close control, and the download
   * path the message already owns.
   */
  import Icon from '$lib/icons/Icon.svelte';
  import { Download, X } from '$lib/icons';
  import { t } from '$lib/i18n';
  import { pushBack } from '$lib/platform/backstack';
  import { handleModalTabKey } from '$lib/platform/focusTrap';
  import {
    classifySwipe,
    clampPan,
    computePinchScale,
    dismissOpacity,
    zoomAroundPoint,
    type Bounds,
    type PanOffset,
  } from '$lib/platform/viewerGestures';
  import { contextMenu } from '$lib/stores/contextmenu.svelte';

  interface Props {
    src: string;
    alt: string;
    onclose: () => void;
    /** Saves the file through the caller's download path; omitted when the caller cannot save. */
    ondownload?: (() => void) | null;
  }

  let { src, alt, onclose, ondownload = null }: Props = $props();

  let rootEl = $state<HTMLDivElement | null>(null);
  let stageEl = $state<HTMLDivElement | null>(null);
  let imgEl = $state<HTMLImageElement | null>(null);
  let closeEl = $state<HTMLButtonElement | null>(null);
  let opener: Element | null = null;

  let scale = $state(1);
  let pan = $state<PanOffset>({ x: 0, y: 0 });
  let dismissY = $state(0);
  let interacting = $state(false);
  let suppressScrimClick = false;
  let lastTap: { x: number; y: number; at: number } | null = null;

  const pointers = new Map<number, { x: number; y: number }>();
  type GestureMode = 'none' | 'pan' | 'dismiss' | 'pinch';
  let mode: GestureMode = 'none';
  let singleStart = { x: 0, y: 0, time: 0 };
  let singlePrev = { x: 0, y: 0, time: 0 };
  let singleLast = { x: 0, y: 0, time: 0 };
  let panStart: PanOffset = { x: 0, y: 0 };
  let pinchStartDistance = 0;
  let pinchStartScale = 1;
  let pinchStartPan: PanOffset = { x: 0, y: 0 };
  let pinchStartMid: PanOffset = { x: 0, y: 0 };
  let startedOnImage = false;

  const scrimOpacity = $derived(dismissY > 0 ? dismissOpacity(dismissY) : undefined);
  const imageTransform = $derived(
    scale !== 1 || pan.x !== 0 || pan.y !== 0 || dismissY !== 0
      ? `translate3d(${pan.x}px, ${pan.y + dismissY}px, 0) scale(${scale})`
      : undefined
  );

  $effect(() => {
    void src;
    scale = 1;
    pan = { x: 0, y: 0 };
    dismissY = 0;
    interacting = false;
    mode = 'none';
    pointers.clear();
  });

  // Android Back closes the lightbox before leaving the screen.
  $effect(() => pushBack(() => onclose()));

  $effect(() => {
    // Remember where the keyboard was, focus the close control, and give it back on the way out.
    opener = document.activeElement;
    closeEl?.focus();
    return () => {
      if (opener instanceof HTMLElement) opener.focus();
    };
  });

  $effect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (contextMenu.isOpen) return;
        event.preventDefault();
        event.stopPropagation();
        onclose();
        return;
      }
      // Keep Tab inside the overlay: it is modal, and the page behind it is not reachable by pointer.
      if (handleModalTabKey(event, rootEl, document.activeElement)) {
        event.preventDefault();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  $effect(() => {
    const el = rootEl;
    if (!el) return;
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  function getBounds(): Bounds {
    return {
      containerWidth: rootEl?.clientWidth ?? window.innerWidth,
      containerHeight: rootEl?.clientHeight ?? window.innerHeight,
      contentWidth: imgEl?.offsetWidth ?? 0,
      contentHeight: imgEl?.offsetHeight ?? 0,
    };
  }

  function pointRelativeToCenter(clientX: number, clientY: number): PanOffset {
    const rect = stageEl?.getBoundingClientRect() ?? rootEl?.getBoundingClientRect();
    if (!rect) return { x: 0, y: 0 };
    return {
      x: clientX - (rect.left + rect.width / 2),
      y: clientY - (rect.top + rect.height / 2),
    };
  }

  function pointerPairMetrics() {
    const [a, b] = Array.from(pointers.values());
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    return {
      distance: Math.hypot(dx, dy),
      midX: (a.x + b.x) / 2,
      midY: (a.y + b.y) / 2,
    };
  }

  function beginSinglePointer(x: number, y: number, time: number) {
    singleStart = { x, y, time };
    singlePrev = { x, y, time };
    singleLast = { x, y, time };
    panStart = { ...pan };
    mode = 'none';
  }

  function toggleZoomAt(clientX: number, clientY: number) {
    if (scale > 1) {
      scale = 1;
      pan = { x: 0, y: 0 };
      return;
    }
    const nextScale = 2;
    const zoomed = zoomAroundPoint(pan, scale, nextScale, pointRelativeToCenter(clientX, clientY));
    scale = nextScale;
    pan = clampPan(zoomed, nextScale, getBounds());
  }

  function onWheel(event: WheelEvent) {
    if (event.target instanceof Element && event.target.closest('.controls')) return;
    event.preventDefault();
    const factor = Math.exp(-event.deltaY * 0.004);
    const nextScale = Math.min(4, Math.max(1, scale * factor));
    if (nextScale <= 1.01) {
      scale = 1;
      pan = { x: 0, y: 0 };
      return;
    }
    const zoomed = zoomAroundPoint(pan, scale, nextScale, pointRelativeToCenter(event.clientX, event.clientY));
    scale = nextScale;
    pan = clampPan(zoomed, nextScale, getBounds());
  }

  function onDblClick(event: MouseEvent) {
    if (event.target instanceof Element && event.target.closest('.controls')) return;
    if (!(event.target instanceof Node) || !imgEl?.contains(event.target)) return;
    toggleZoomAt(event.clientX, event.clientY);
  }

  function onPointerDown(event: PointerEvent) {
    if (event.target instanceof Element && event.target.closest('.controls')) return;
    if (event.pointerType === 'mouse' && event.button !== 0) return;
    if (pointers.size === 0) {
      suppressScrimClick = false;
      startedOnImage = !!(event.target instanceof Node && imgEl?.contains(event.target));
    }
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });

    if (pointers.size === 1) {
      beginSinglePointer(event.clientX, event.clientY, event.timeStamp);
      interacting = false;
    } else if (pointers.size === 2) {
      const { distance, midX, midY } = pointerPairMetrics();
      mode = 'pinch';
      dismissY = 0;
      interacting = true;
      suppressScrimClick = true;
      pinchStartDistance = Math.max(1, distance);
      pinchStartScale = scale;
      pinchStartPan = { ...pan };
      pinchStartMid = { x: midX, y: midY };
    }
  }

  function onPointerMove(event: PointerEvent) {
    if (!pointers.has(event.pointerId)) return;
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });

    if (mode === 'pinch' && pointers.size >= 2) {
      const { distance, midX, midY } = pointerPairMetrics();
      const nextScale = computePinchScale(pinchStartScale, pinchStartDistance, distance, 1, 4);
      const zoomed = zoomAroundPoint(
        pinchStartPan,
        pinchStartScale,
        nextScale,
        pointRelativeToCenter(pinchStartMid.x, pinchStartMid.y)
      );
      const translated: PanOffset = {
        x: zoomed.x + (midX - pinchStartMid.x),
        y: zoomed.y + (midY - pinchStartMid.y),
      };
      scale = nextScale;
      pan = nextScale <= 1 ? { x: 0, y: 0 } : clampPan(translated, nextScale, getBounds());
      return;
    }

    if (pointers.size !== 1) return;
    const dx = event.clientX - singleStart.x;
    const dy = event.clientY - singleStart.y;

    if (mode === 'none') {
      if (Math.hypot(dx, dy) < 6) return;
      if (scale > 1) {
        mode = 'pan';
      } else if ((startedOnImage || event.pointerType !== 'mouse') && dy > 0 && Math.abs(dy) > Math.abs(dx)) {
        mode = 'dismiss';
      } else {
        return;
      }
      interacting = true;
      suppressScrimClick = true;
      try {
        rootEl?.setPointerCapture(event.pointerId);
      } catch {
        /* pointer already released */
      }
    }

    if (mode === 'pan') {
      pan = clampPan({ x: panStart.x + dx, y: panStart.y + dy }, scale, getBounds());
    } else if (mode === 'dismiss') {
      dismissY = Math.max(0, dy);
    }

    if (event.clientX !== singleLast.x || event.clientY !== singleLast.y) {
      singlePrev = singleLast;
      singleLast = { x: event.clientX, y: event.clientY, time: event.timeStamp };
    }
  }

  function onPointerEnd(event: PointerEvent, cancelled: boolean) {
    if (!pointers.has(event.pointerId)) return;
    pointers.delete(event.pointerId);

    try {
      if (rootEl?.hasPointerCapture(event.pointerId)) rootEl.releasePointerCapture(event.pointerId);
    } catch {
      /* ignore */
    }

    if (mode === 'pinch') {
      if (pointers.size === 1) {
        const [remaining] = Array.from(pointers.values());
        beginSinglePointer(remaining.x, remaining.y, event.timeStamp);
        if (scale > 1) mode = 'pan';
      } else if (pointers.size === 0) {
        mode = 'none';
        interacting = false;
      }
      return;
    }

    const finishedMode = mode;
    mode = 'none';
    interacting = false;

    if (finishedMode === 'dismiss') {
      if (!cancelled) {
        const dx = event.clientX - singleStart.x;
        const dy = event.clientY - singleStart.y;
        const sampleRef =
          event.clientX === singleLast.x && event.clientY === singleLast.y ? singlePrev : singleLast;
        const dt = Math.max(1, event.timeStamp - sampleRef.time);
        const vx = (event.clientX - sampleRef.x) / dt;
        const vy = (event.clientY - sampleRef.y) / dt;
        if (classifySwipe(dx, dy, vx, vy) === 'dismiss') {
          onclose();
          return;
        }
      }
      dismissY = 0;
      return;
    }

    if (finishedMode === 'pan') {
      pan = clampPan(pan, scale, getBounds());
      return;
    }

    if (
      !cancelled &&
      event.pointerType !== 'mouse' &&
      finishedMode === 'none' &&
      event.target instanceof Node &&
      imgEl?.contains(event.target)
    ) {
      const now = event.timeStamp;
      if (
        lastTap &&
        now - lastTap.at < 300 &&
        Math.hypot(event.clientX - lastTap.x, event.clientY - lastTap.y) <= 24
      ) {
        lastTap = null;
        toggleZoomAt(event.clientX, event.clientY);
      } else {
        lastTap = { x: event.clientX, y: event.clientY, at: now };
      }
    }
  }

  function onScrimClick() {
    if (suppressScrimClick) {
      suppressScrimClick = false;
      return;
    }
    onclose();
  }
</script>

<!-- The backdrop is a real control so clicking it needs no handler on a static element; the picture
     and the controls sit above it. -->
<div
  class="lightbox"
  class:interacting
  role="dialog"
  aria-modal="true"
  aria-label={alt}
  tabindex="-1"
  bind:this={rootEl}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={(e) => onPointerEnd(e, false)}
  onpointercancel={(e) => onPointerEnd(e, true)}
  ondblclick={onDblClick}
>
  <!-- The backdrop closes on click but is not a second tab stop: Escape and the close control do
       the same job for the keyboard, and two controls named "Close" only add noise. -->
  <button
    type="button"
    class="scrim"
    tabindex="-1"
    aria-label={t('common-close')}
    style:opacity={scrimOpacity}
    onclick={onScrimClick}
  ></button>
  <div class="stage" bind:this={stageEl}>
    <img
      bind:this={imgEl}
      {src}
      {alt}
      class:zoomed={scale > 1}
      class:interacting
      style:transform={imageTransform}
    />
  </div>
  <div class="controls">
    {#if ondownload}
      <button
        type="button"
        class="control"
        aria-label={t('media-download', { name: alt })}
        data-tooltip={t('media-view-download')}
        onclick={() => ondownload?.()}
      >
        <Icon icon={Download} size={20} />
      </button>
    {/if}
    <button
      type="button"
      class="control close"
      bind:this={closeEl}
      aria-label={t('common-close')}
      data-tooltip={t('common-close')}
      onclick={onclose}
    >
      <Icon icon={X} size={20} />
    </button>
  </div>
</div>

<style>
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: var(--alloy-z-lightbox, 80);
    display: grid;
    place-items: center;
    padding: var(--alloy-spacing-scale-6);
    overflow: hidden;
    touch-action: none;
    outline: 0;
  }
  .scrim {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 92%, transparent);
    -webkit-backdrop-filter: blur(12px);
    backdrop-filter: blur(12px);
    cursor: default;
    transition: opacity var(--alloy-motion-duration-normal) ease-out;
  }
  .lightbox.interacting .scrim {
    transition: none;
  }
  .scrim:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -4px;
  }
  .stage {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    max-width: 100%;
    max-height: 100%;
    touch-action: none;
  }
  img {
    display: block;
    max-width: min(92vw, 1600px);
    max-height: 86vh;
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-glass-shadow-soft);
    /* The picture is content, not a control: no selection handles, no drag ghost. */
    user-select: none;
    -webkit-user-drag: none;
    transform-origin: center center;
    transition: transform var(--alloy-motion-duration-normal) ease-out;
  }
  img.zoomed {
    cursor: grab;
  }
  img.interacting {
    transition: none;
  }
  img.zoomed.interacting {
    cursor: grabbing;
  }
  .controls {
    position: absolute;
    z-index: 2;
    top: var(--alloy-spacing-scale-4);
    right: var(--alloy-spacing-scale-4);
    display: flex;
    gap: var(--alloy-spacing-scale-2);
  }
  @media (prefers-reduced-motion: reduce) {
    .scrim,
    img {
      transition: none;
    }
  }
  :global(:root[data-reduced-motion='true']) .scrim,
  :global(:root[data-reduced-motion='true']) img {
    transition: none;
  }
  .control {
    display: inline-grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-fill-strong);
    color: var(--alloy-color-text-primary);
    cursor: pointer;
  }
  .control:hover {
    background: var(--alloy-glass-control-hover);
  }
  .control:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  @media (max-width: 899px) {
    .lightbox {
      padding: var(--alloy-spacing-scale-2);
    }
    .control {
      width: 44px;
      height: 44px;
    }
    img {
      max-height: 80vh;
    }
  }
</style>
