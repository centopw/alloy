<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { X } from '$lib/icons';
  import { pushBack } from '$lib/platform/backstack';

  interface Props {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: string;
  }
  let { open, title, onclose, children, width = '520px' }: Props = $props();

  let dialog: HTMLDivElement | undefined = $state();
  let opener: HTMLElement | null = null;

  // Android Back closes the dialog.
  $effect(() => {
    if (!open) return;
    return pushBack(() => onclose());
  });

  $effect(() => {
    if (open) {
      opener = document.activeElement as HTMLElement | null;
      void tick().then(() => dialog?.focus());
    } else {
      opener?.focus?.();
      opener = null;
    }
  });

  // Phone bottom sheet: dragging the grab bar down past DISMISS_PX closes the sheet.
  const DISMISS_PX = 80;
  let dragY = $state(0);
  let dragStartY: number | null = null;

  function onGrabDown(e: PointerEvent) {
    dragStartY = e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onGrabMove(e: PointerEvent) {
    if (dragStartY === null) return;
    dragY = Math.max(0, e.clientY - dragStartY);
  }

  function onGrabEnd() {
    if (dragStartY === null) return;
    dragStartY = null;
    const dismiss = dragY >= DISMISS_PX;
    dragY = 0;
    if (dismiss) onclose();
  }

  function onGrabCancel() {
    dragStartY = null;
    dragY = 0;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    } else if (e.key === 'Tab' && dialog) {
      // Keep focus inside the dialog.
      const f = dialog.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
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
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={onclose}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      style="--alloy-modal-width: min({width}, 94vw);"
      style:transform={dragY ? `translateY(${dragY}px)` : undefined}
      bind:this={dialog}
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKey}
    >
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="grab"
        aria-hidden="true"
        onpointerdown={onGrabDown}
        onpointermove={onGrabMove}
        onpointerup={onGrabEnd}
        onpointercancel={onGrabCancel}
      >
        <span class="grab-bar"></span>
      </div>
      <header>
        <h2>{title}</h2>
        <button type="button" aria-label="Close" onclick={onclose}><Icon icon={X} size={20} /></button>
      </header>
      <div class="content scroll-y">{@render children()}</div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--alloy-color-scrim);
    -webkit-backdrop-filter: blur(10px) saturate(140%);
    backdrop-filter: blur(10px) saturate(140%);
    display: grid;
    place-items: center;
    /* Above Settings (60) so a dialog opened from it is visible at once; below the media viewer (70). */
    z-index: 65;
  }
  .dialog {
    width: var(--alloy-modal-width);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    color: var(--alloy-color-text-primary);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-shell);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
    outline: 0;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-glass-rim);
  }
  h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
  }
  header button {
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
    border-radius: var(--alloy-radii-full);
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
  }
  header button:hover {
    background: var(--alloy-glass-control);
  }
  .content {
    padding: var(--alloy-layout-gutter);
    overflow-y: auto;
  }

  /* Grab zone: drag handle of the phone bottom sheet. Hidden on desktop. */
  .grab {
    display: none;
  }
  .grab-bar {
    width: 36px;
    height: 4px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-border-prominent);
  }

  /* Phone (<= 899px): bottom sheet, full width, at most 90% of the viewport, rounded top corners.
     Pinned to the bottom edge and padded above the home indicator. */
  @media (max-width: 899px) {
    .backdrop {
      place-items: end stretch;
      animation: alloy-fade-in var(--alloy-motion-duration-normal) ease-out backwards;
    }
    .dialog {
      width: 100%;
      max-width: 100%;
      max-height: 90dvh;
      border-left: 0;
      border-right: 0;
      border-bottom: 0;
      border-radius: var(--alloy-glass-radius-panel) var(--alloy-glass-radius-panel) 0 0;
      padding-bottom: var(--alloy-safe-area-bottom);
      animation: alloy-sheet-up var(--alloy-motion-duration-normal) ease-out backwards;
    }
    .grab {
      display: flex;
      justify-content: center;
      align-items: center;
      flex: none;
      height: 24px;
      touch-action: none;
      cursor: grab;
    }
    header button {
      width: var(--alloy-touch-target);
      height: var(--alloy-touch-target);
    }
    .content {
      flex: 1 1 auto;
      min-height: 0;
    }
  }

  @keyframes alloy-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes alloy-sheet-up {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
</style>
