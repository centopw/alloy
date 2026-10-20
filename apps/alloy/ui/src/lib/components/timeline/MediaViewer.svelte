<script module lang="ts">
  export { fetchMediaFile } from '$lib/content/mediaFetch';
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Download, X, ChevronLeft, ChevronRight } from '$lib/icons';
  import { errorMessage } from '$lib/stores/errors';
  import { pushBack } from '$lib/platform/backstack';
  import { portal } from '$lib/platform/portal';
  import { canDownloadMedia, downloadMedia } from '$lib/platform/saveFile';
  import { displayNameFor } from '$lib/platform/names';
  import { formatTime } from '$lib/content/format';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { fetchMediaFile } from '$lib/content/mediaFetch';
  import type { MediaItem } from '$lib/content/mediaItems';
  import {
    classifySwipe,
    computePinchScale,
    clampPan,
    zoomAroundPoint,
    dismissOpacity,
  } from '$lib/platform/viewerGestures';

  interface Props {
    open: boolean;
    onclose: () => void;
    items: MediaItem[];
    index: number;
    /** Optional test/harness resolver to bypass engine fetching. */
    resolve?: (uri: string, mimeType: string) => Promise<string>;
  }

  let { open, onclose, items, index, resolve }: Props = $props();

  let currentIndex = $state(0);
  $effect(() => {
    if (open) {
      currentIndex = index;
    }
  });

  let src = $state<string | null>(null);
  let failed = $state(false);
  let saving = $state(false);
  let status = $state<string | null>(null);
  let root: HTMLDivElement | undefined = $state();
  let opener: HTMLElement | null = null;
  let stageEl: HTMLDivElement | undefined = $state();
  let imgEl: HTMLImageElement | undefined = $state();
  let videoEl: HTMLVideoElement | undefined = $state();

  // Gesture state
  let scale = $state(1);
  let pan = $state({ x: 0, y: 0 });
  let dragOffset = $state({ x: 0, y: 0 });
  let isDragging = $state(false);
  let dismissDrag = $state(0);
  let dismissBackdrop = $derived(dismissOpacity(dismissDrag));

  let tapCount = 0;
  let tapTimer: number | null = null;

  const currentItem = $derived(items[currentIndex]);
  const isImage = $derived(currentItem?.mediaKind === 'image');
  const isVideo = $derived(currentItem?.mediaKind === 'video');
  const canGoNext = $derived(currentIndex < items.length - 1);
  const canGoPrev = $derived(currentIndex > 0);
  const title = $derived(
    currentItem?.body || (isVideo ? 'Video' : isImage ? 'Image' : 'Media')
  );
  const senderName = $derived(
    displayNameFor(currentItem?.sender || '', currentItem?.senderName)
  );
  const timestamp = $derived(currentItem?.timestampMillis ?? 0);
  const positionText = $derived(
    items.length > 1 ? `${currentIndex + 1} / ${items.length}` : ''
  );

  const downloadable = canDownloadMedia();
  const reduceMotion = $derived(
    typeof matchMedia !== 'undefined' &&
    matchMedia('(prefers-reduced-motion: reduce)').matches
  );

  // Load the full file only while the viewer is open and index changes.
  $effect(() => {
    if (!open || !currentItem) return;
    let cancelled = false;
    src = null;
    failed = false;
    status = null;
    scale = 1;
    pan = { x: 0, y: 0 };
    dragOffset = { x: 0, y: 0 };
    dismissDrag = 0;

    const resolver = resolve ?? fetchMediaFile;
    resolver(currentItem.uri, currentItem.mimeType)
      .then((url) => {
        if (!cancelled) src = url;
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });

    // Preload neighbors in background
    if (canGoNext && items[currentIndex + 1]) {
      const nextItem = items[currentIndex + 1];
      resolver(nextItem.uri, nextItem.mimeType).catch(() => {});
    }
    if (canGoPrev && items[currentIndex - 1]) {
      const prevItem = items[currentIndex - 1];
      resolver(prevItem.uri, prevItem.mimeType).catch(() => {});
    }

    return () => {
      cancelled = true;
    };
  });

  // Android Back closes the viewer.
  $effect(() => {
    if (!open) return;
    return pushBack(() => onclose());
  });

  $effect(() => {
    if (open) {
      opener = document.activeElement as HTMLElement | null;
      void tick().then(() => root?.focus());
    } else {
      opener?.focus?.();
      opener = null;
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      goToPrev();
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      goToNext();
    } else if (e.key === 'Tab' && root) {
      // Keep focus inside the viewer.
      const f = root.querySelectorAll<HTMLElement>('button:not(:disabled), video, audio');
      if (f.length === 0) return;
      const first = f[0];
      const last = f[f.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  function goToNext() {
    if (canGoNext) currentIndex++;
  }

  function goToPrev() {
    if (canGoPrev) currentIndex--;
  }

  async function download() {
    if (!currentItem) return;
    saving = true;
    status = null;
    try {
      if (await downloadMedia(currentItem.uri, currentItem.mimeType, title)) {
        status = 'Saved';
      }
    } catch (err) {
      status = errorMessage(err);
    } finally {
      saving = false;
    }
  }

  function toggleZoom(clientX: number, clientY: number) {
    if (!imgEl || !stageEl || isVideo) return;
    const containerRect = stageEl.getBoundingClientRect();
    const rect = imgEl.getBoundingClientRect();
    const focalX = clientX - containerRect.left - containerRect.width / 2;
    const focalY = clientY - containerRect.top - containerRect.height / 2;

    if (scale === 1) {
      scale = 2;
      pan = zoomAroundPoint(pan, 1, 2, { x: focalX, y: focalY });
      pan = clampPan(pan, 2, {
        containerWidth: containerRect.width,
        containerHeight: containerRect.height,
        contentWidth: rect.width,
        contentHeight: rect.height,
      });
    } else {
      scale = 1;
      pan = { x: 0, y: 0 };
    }
  }

  function onDblClick(e: MouseEvent) {
    if (e.target !== imgEl && e.target !== stageEl) return;
    toggleZoom(e.clientX, e.clientY);
  }

  // Pointer gesture tracking
  const pointerMap = new Map<number, { x: number; y: number }>();
  let dragStart = { x: 0, y: 0 };
  let dragStartTime = 0;
  let initialPinchDistance = 0;
  let pinchStartScale = 1;

  function onPointerDown(e: PointerEvent) {
    if (isVideo && e.target !== stageEl) return;
    if (!stageEl) return;

    pointerMap.set(e.pointerId, { x: e.clientX, y: e.clientY });
    isDragging = true;

    if (pointerMap.size === 1) {
      dragStart = { x: e.clientX, y: e.clientY };
      dragStartTime = performance.now();

      // Double-tap zoom on touch
      if (e.pointerType === 'touch') {
        tapCount++;
        if (tapTimer !== null) clearTimeout(tapTimer);
        if (tapCount === 2) {
          tapCount = 0;
          toggleZoom(e.clientX, e.clientY);
        } else {
          tapTimer = window.setTimeout(() => {
            tapCount = 0;
          }, 300);
        }
      }
    } else if (pointerMap.size === 2 && !isVideo) {
      // Start pinch
      const [p1, p2] = Array.from(pointerMap.values());
      initialPinchDistance = Math.hypot(p2.x - p1.x, p2.y - p1.y);
      pinchStartScale = scale;
      dragOffset = { x: 0, y: 0 };
      dismissDrag = 0;
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!pointerMap.has(e.pointerId) || !stageEl) return;

    const prevPos = pointerMap.get(e.pointerId) || { x: e.clientX, y: e.clientY };
    pointerMap.set(e.pointerId, { x: e.clientX, y: e.clientY });

    if (pointerMap.size === 1) {
      const dx = e.clientX - dragStart.x;
      const dy = e.clientY - dragStart.y;

      if (scale === 1) {
        // Zoom 1: swipe horizontally to change item, or downward to dismiss
        if (dismissDrag > 0 || (dy > 0 && Math.abs(dy) > Math.abs(dx))) {
          dismissDrag = Math.max(0, dy);
          dragOffset = { x: 0, y: dismissDrag };
        } else {
          dragOffset = { x: dx, y: 0 };
          dismissDrag = 0;
        }
      } else if (!isVideo && imgEl) {
        // Zoomed: pan image clamped to bounds
        const deltaX = e.clientX - prevPos.x;
        const deltaY = e.clientY - prevPos.y;
        const containerRect = stageEl.getBoundingClientRect();
        const rect = imgEl.getBoundingClientRect();

        pan = clampPan(
          { x: pan.x + deltaX, y: pan.y + deltaY },
          scale,
          {
            containerWidth: containerRect.width,
            containerHeight: containerRect.height,
            contentWidth: rect.width,
            contentHeight: rect.height,
          }
        );
      }
    } else if (pointerMap.size === 2 && !isVideo && imgEl) {
      // Two-pointer pinch zoom
      const [p1, p2] = Array.from(pointerMap.values());
      const currentDist = Math.hypot(p2.x - p1.x, p2.y - p1.y);
      const newScale = computePinchScale(pinchStartScale, initialPinchDistance, currentDist);

      const containerRect = stageEl.getBoundingClientRect();
      const rect = imgEl.getBoundingClientRect();
      const midX = (p1.x + p2.x) / 2 - containerRect.left - containerRect.width / 2;
      const midY = (p1.y + p2.y) / 2 - containerRect.top - containerRect.height / 2;

      pan = zoomAroundPoint(pan, scale, newScale, { x: midX, y: midY });
      scale = newScale;
      pan = clampPan(pan, scale, {
        containerWidth: containerRect.width,
        containerHeight: containerRect.height,
        contentWidth: rect.width,
        contentHeight: rect.height,
      });
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!pointerMap.has(e.pointerId)) return;
    pointerMap.delete(e.pointerId);

    if (pointerMap.size === 0) {
      isDragging = false;
      initialPinchDistance = 0;

      if (scale === 1) {
        const dx = e.clientX - dragStart.x;
        const dy = e.clientY - dragStart.y;
        const elapsed = Math.max(1, performance.now() - dragStartTime);
        const vx = dx / elapsed;
        const vy = dy / elapsed;

        const action = classifySwipe(dx, dy, vx, vy);
        if (action === 'dismiss') {
          onclose();
          return;
        } else if (action === 'horizontal-next') {
          goToNext();
        } else if (action === 'horizontal-prev') {
          goToPrev();
        }

        // Spring back if not dismissed or navigated
        dragOffset = { x: 0, y: 0 };
        dismissDrag = 0;
      }
    }
  }

  function onPointerCancel(e: PointerEvent) {
    pointerMap.delete(e.pointerId);
    if (pointerMap.size === 0) {
      isDragging = false;
      initialPinchDistance = 0;
      dragOffset = { x: 0, y: 0 };
      dismissDrag = 0;
    }
  }

  function onWheel(e: WheelEvent) {
    if (!imgEl || !stageEl || isVideo) return;
    if (!e.ctrlKey) return;
    e.preventDefault();

    const rect = imgEl.getBoundingClientRect();
    const containerRect = stageEl.getBoundingClientRect();
    const focalX = e.clientX - containerRect.left - containerRect.width / 2;
    const focalY = e.clientY - containerRect.top - containerRect.height / 2;
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    const newScale = computePinchScale(scale, 1, delta);

    pan = zoomAroundPoint(pan, scale, newScale, { x: focalX, y: focalY });
    scale = newScale;
    pan = clampPan(pan, scale, {
      containerWidth: containerRect.width,
      containerHeight: containerRect.height,
      contentWidth: rect.width,
      contentHeight: rect.height,
    });
  }
</script>

{#if open && currentItem}
  <div
    class="viewer"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    bind:this={root}
    onkeydown={onKey}
    use:portal
  >
    <header class="bar">
      <h2 class="name">{title}</h2>
      <p class="status" aria-live="polite">{status ?? ''}</p>
      {#if downloadable}
        <button
          type="button"
          class="tool"
          aria-label="Download"
          data-tooltip="Download"
          onclick={download}
          disabled={saving || src === null}
        >
          <Icon icon={Download} size={20} />
        </button>
      {/if}
      <button type="button" class="tool" aria-label="Close" data-tooltip="Close" onclick={onclose}>
        <Icon icon={X} size={20} />
      </button>
    </header>

    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="stage"
      bind:this={stageEl}
      style="opacity: {dismissBackdrop}"
      onclick={(e) => e.target === e.currentTarget && onclose()}
      ondblclick={onDblClick}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerCancel}
      onwheel={onWheel}
    >
      {#if failed}
        <p class="note" role="alert">This {currentItem.mediaKind} could not be loaded.</p>
      {:else if src === null}
        <p class="note" aria-live="polite">Loading</p>
      {:else if isVideo}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video
          bind:this={videoEl}
          controls
          autoplay
          src={src}
          aria-label={title}
          style="
            transform: translate({dragOffset.x}px, {dragOffset.y}px);
            transition: {isDragging || reduceMotion ? 'none' : 'transform var(--alloy-motion-duration-normal) ease-out'};
          "
        ></video>
      {:else if isImage}
        <img
          bind:this={imgEl}
          {src}
          alt={title}
          draggable="false"
          style="
            transform: translate({scale === 1 ? dragOffset.x : pan.x}px, {scale === 1 ? dragOffset.y : pan.y}px) scale({scale});
            transition: {isDragging || reduceMotion ? 'none' : 'transform var(--alloy-motion-duration-normal) ease-out'};
            touch-action: none;
            cursor: {scale > 1 ? (isDragging ? 'grabbing' : 'grab') : 'default'};
          "
        />
      {/if}

      {#if canGoPrev}
        <button
          type="button"
          class="nav prev"
          aria-label="Previous"
          data-tooltip="Previous"
          onclick={goToPrev}
        >
          <Icon icon={ChevronLeft} size={24} />
        </button>
      {/if}

      {#if canGoNext}
        <button
          type="button"
          class="nav next"
          aria-label="Next"
          data-tooltip="Next"
          onclick={goToNext}
        >
          <Icon icon={ChevronRight} size={24} />
        </button>
      {/if}
    </div>

    {#if !failed || positionText}
      <footer class="footer">
        <div class="info">
          <span class="sender">{senderName}</span>
          <span class="time">{formatTime(timestamp, prefs.timeFormat)}</span>
        </div>
        {#if positionText}
          <span class="position">{positionText}</span>
        {/if}
      </footer>
    {/if}
  </div>
{/if}

<style>
  /* Fills the whole window on every platform (portalled out of the virtualised timeline, whose transforms would otherwise confine it); the media keeps its own shape, with no panel behind it. */
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: flex;
    flex-direction: column;
    padding-top: var(--alloy-safe-area-top);
    padding-bottom: var(--alloy-safe-area-bottom);
    background: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
    outline: none;
  }

  .bar {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
  }

  .name {
    flex: 1 1 auto;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    flex: none;
    margin: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }

  .tool {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }

  .tool:hover:not(:disabled) {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  .tool:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .tool:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }

  .stage {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-3);
    position: relative;
    user-select: none;
  }

  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    transform-origin: center;
    user-select: none;
  }

  video {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  .nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    cursor: pointer;
    opacity: 0.8;
  }

  .nav:hover {
    opacity: 1;
    background: var(--alloy-color-surface-hover);
  }

  .nav:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }

  .nav.prev {
    left: var(--alloy-spacing-scale-3);
  }

  .nav.next {
    right: var(--alloy-spacing-scale-3);
  }

  .footer {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .sender {
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-secondary);
  }

  .position {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    flex: none;
  }

  .note {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    text-align: center;
  }
</style>
