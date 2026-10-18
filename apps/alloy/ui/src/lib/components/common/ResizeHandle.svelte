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
    onchange: (value: number) => void;
    /** Called once a drag ends or a keyboard/reset change is applied, for persistence. */
    ondone?: () => void;
  }

  let { value, min, max, label, edge, defaultValue, onchange, ondone }: Props = $props();

  const KEY_STEP = 16;
  const KEY_STEP_LARGE = 64;

  let handleEl: HTMLDivElement | undefined = $state();
  let drag: { startX: number; startValue: number } | null = null;

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
    handleEl.setPointerCapture(e.pointerId);
    drag = { startX: e.clientX, startValue: value };
    document.body.classList.add('alloy-resizing');
  }

  function onPointerMove(e: PointerEvent) {
    if (drag === null) return;
    const dx = e.clientX - drag.startX;
    onchange(clamp(drag.startValue + (edge === 'end' ? dx : -dx)));
  }

  function endDrag() {
    if (drag === null) return;
    drag = null;
    document.body.classList.remove('alloy-resizing');
    ondone?.();
  }

  function onKeyDown(e: KeyboardEvent) {
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
    if (defaultValue !== undefined) commit(defaultValue);
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
  aria-valuenow={Math.round(value)}
  aria-valuemin={min}
  aria-valuemax={max}
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
    flex: 0 0 6px;
    align-self: stretch;
    cursor: col-resize;
    touch-action: none;
    user-select: none;
    outline: none;
  }

  .handle::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 2px;
    transform: translateX(-50%);
    background: transparent;
    transition: background var(--alloy-motion-duration-fast) ease-out;
  }

  .handle:hover::before,
  .handle:focus-visible::before {
    background: var(--alloy-color-accent-primary);
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

  @media (prefers-reduced-motion: reduce) {
    .handle::before {
      transition: none;
    }
  }
</style>
