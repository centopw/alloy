<script lang="ts">
  interface Props {
    /** Current width of the panel this handle resizes. */
    value: number;
    min: number;
    max: number;
    /** Accessible name, e.g. "Resize sidebar". */
    label: string;
    /**
     * Which side of the panel the handle sits on. 'end': the panel is to the handle's left, so
     * dragging right grows it. 'start': the panel is to the handle's right, so dragging left grows it.
     */
    edge: 'start' | 'end';
    /** Width applied on double-click. Omit to disable double-click reset. */
    defaultValue?: number;
    /**
     * Drag threshold below the minimum that collapses the panel instead of clamping at the minimum.
     * Omit when the panel cannot collapse.
     */
    collapseBelow?: number;
    onchange: (value: number) => void;
    /** Called once a drag ends or a keyboard/reset change is applied, for persistence. */
    ondone?: () => void;
    /** Called when the panel is dragged past `collapseBelow`. */
    oncollapse?: () => void;
    /**
     * True while the panel is hidden. The handle stays on the boundary it shares with the rail, so
     * the same edge that collapsed the panel is the one that brings it back.
     */
    collapsed?: boolean;
    /** Called when a drag, key press or double-click starts from the collapsed state. */
    onexpand?: () => void;
  }

  let {
    value,
    min,
    max,
    label,
    edge,
    defaultValue,
    collapseBelow,
    onchange,
    ondone,
    oncollapse,
    collapsed = false,
    onexpand,
  }: Props = $props();

  const KEY_STEP = 16;
  const KEY_STEP_LARGE = 64;

  let handleEl: HTMLDivElement | undefined = $state();
  let drag: { startX: number; startValue: number } | null = null;
  let collapsing = false;

  function clamp(v: number): number {
    return Math.min(max, Math.max(min, v));
  }

  function commit(v: number) {
    onchange(clamp(v));
    ondone?.();
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || !handleEl) return;
    e.preventDefault();
    // Grabbing the edge of a hidden panel opens it at its stored width, and the same drag keeps
    // resizing from there: one gesture, no separate "open" step.
    if (collapsed) onexpand?.();
    handleEl.setPointerCapture(e.pointerId);
    drag = { startX: e.clientX, startValue: value };
    collapsing = false;
    document.body.classList.add('alloy-resizing');
  }

  function onPointerMove(e: PointerEvent) {
    if (drag === null) return;
    const dx = e.clientX - drag.startX;
    const raw = drag.startValue + (edge === 'end' ? dx : -dx);
    // Past the collapse threshold the panel hides; the drag ends with it.
    if (collapseBelow !== undefined && raw <= collapseBelow) {
      if (!collapsing) {
        collapsing = true;
        // Put the width back where the drag began, so expanding later does not restore the
        // minimum width the drag passed through on its way to the collapse threshold.
        onchange(clamp(drag.startValue));
        oncollapse?.();
      }
      return;
    }
    onchange(clamp(raw));
  }

  function endDrag() {
    if (drag === null) return;
    drag = null;
    document.body.classList.remove('alloy-resizing');
    ondone?.();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (collapsed) {
      // A hidden panel only has one useful direction; the other arrow is a no-op.
      if (e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') {
        e.preventDefault();
        onexpand?.();
        commit(e.key === 'End' ? max : e.key === 'Home' ? min : value);
      }
      return;
    }
    const step = e.shiftKey ? KEY_STEP_LARGE : KEY_STEP;
    const sign = edge === 'end' ? 1 : -1;
    switch (e.key) {
      case 'ArrowLeft':
        e.preventDefault();
        commit(value - sign * step);
        break;
      case 'ArrowRight':
        e.preventDefault();
        commit(value + sign * step);
        break;
      case 'Home':
        e.preventDefault();
        commit(min);
        break;
      case 'End':
        e.preventDefault();
        commit(max);
        break;
    }
  }

  function onDoubleClick() {
    if (defaultValue === undefined) return;
    if (collapsed) onexpand?.();
    commit(defaultValue);
  }

  // Remove the body class if the handle is destroyed mid-drag, so the cursor and selection reset.
  $effect(() => () => {
    if (drag !== null) document.body.classList.remove('alloy-resizing');
  });
</script>

<!-- The handle is a focusable WAI-ARIA window splitter (role="separator"), so tabindex and key handling are intentional. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={handleEl}
  class="handle"
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={collapsed ? 0 : Math.round(value)}
  aria-valuemin={collapsed ? 0 : min}
  aria-valuemax={max}
  aria-valuetext={collapsed ? 'Hidden' : undefined}
  tabindex="0"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={endDrag}
  onpointercancel={endDrag}
  onlostpointercapture={endDrag}
  onkeydown={onKeyDown}
  ondblclick={onDoubleClick}
></div>

<style>
  .handle {
    position: relative;
    flex: 1 1 auto;
    align-self: stretch;
    width: 100%;
    cursor: col-resize;
    touch-action: none;
    user-select: none;
    outline: none;
  }

  /* A hairline marks the boundary while hovering; the grip says the line can be dragged. */
  .handle::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 1px;
    transform: translateX(-50%);
    background: transparent;
    transition: background var(--alloy-motion-duration-fast) ease-out;
  }

  .handle::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 50%;
    width: 4px;
    height: 36px;
    transform: translate(-50%, -50%) scaleY(0.7);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    opacity: 0;
    transition:
      opacity var(--alloy-motion-duration-fast) ease-out,
      transform var(--alloy-motion-duration-fast) ease-out;
  }

  .handle:hover::before,
  .handle:focus-visible::before {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 45%, transparent);
  }

  .handle:hover::after,
  .handle:focus-visible::after {
    opacity: 1;
    transform: translate(-50%, -50%) scaleY(1);
  }

  .handle:focus-visible {
    outline: none;
  }

  :global(body.alloy-resizing) {
    cursor: col-resize;
    user-select: none;
  }

  :global(body.alloy-resizing) .handle::before {
    background: var(--alloy-color-accent-primary);
  }

  :global(body.alloy-resizing) .handle::after {
    opacity: 1;
    transform: translate(-50%, -50%) scaleY(1);
  }

  @media (prefers-reduced-motion: reduce) {
    .handle::before,
    .handle::after {
      transition: none;
    }
  }
</style>
