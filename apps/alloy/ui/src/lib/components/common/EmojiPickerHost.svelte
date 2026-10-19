<script lang="ts">
  import EmojiPicker from './EmojiPicker.svelte';
  import { portal } from '$lib/platform/portal';
  import { placeAnchor } from '$lib/platform/contextmenu';
  import { pushBack } from '$lib/platform/backstack';
  import { contextMenu } from '$lib/stores/contextmenu.svelte';
  import { emojiPicker } from '$lib/stores/emojiPicker.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';

  const POPOVER = { width: 360, height: 420 };
  /** Distance a phone sheet must be dragged down before it closes. */
  const DISMISS_DISTANCE = 80;

  /**
   * Set on every pointerdown (capture phase, registered before ContextMenu's), so the backdrop click
   * that follows can tell that the same press already dismissed an open skin-tone menu.
   */
  let menuWasOpenAtPress = false;

  let dragOffset = $state(0);
  let dragStartY = 0;
  let dragging = false;

  let returnFocus: HTMLElement | null = null;

  const popoverPos = $derived.by(() => {
    const anchor = emojiPicker.anchor;
    if (!anchor || viewport.isPhone) return null;
    return placeAnchor(anchor, POPOVER, {
      width: window.innerWidth,
      height: window.innerHeight
    });
  });

  $effect(() => {
    const onPress = () => {
      menuWasOpenAtPress = contextMenu.isOpen;
    };
    window.addEventListener('pointerdown', onPress, true);
    return () => window.removeEventListener('pointerdown', onPress, true);
  });

  $effect(() => {
    if (!emojiPicker.isOpen) return;
    const prior = document.activeElement;
    returnFocus = prior instanceof HTMLElement ? prior : null;
    return () => {
      if (returnFocus?.isConnected) returnFocus.focus();
      returnFocus = null;
    };
  });

  $effect(() => {
    if (!emojiPicker.isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      // Escape belongs to an open skin-tone menu first; ContextMenu handles it.
      if (e.key === 'Escape' && !contextMenu.isOpen) {
        e.preventDefault();
        emojiPicker.close();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  $effect(() => {
    if (!emojiPicker.isOpen) return;
    // Anchors are measured once; any resize would leave the popover misplaced, so close.
    const onResize = () => emojiPicker.close();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });

  $effect(() => {
    if (!emojiPicker.isOpen || !viewport.isPhone) return;
    return pushBack(() => emojiPicker.close());
  });

  function dismissFromBackdrop() {
    if (menuWasOpenAtPress) {
      menuWasOpenAtPress = false;
      return;
    }
    emojiPicker.close();
  }

  function onGripDown(e: PointerEvent) {
    dragging = true;
    dragStartY = e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onGripMove(e: PointerEvent) {
    if (!dragging) return;
    dragOffset = Math.max(0, e.clientY - dragStartY);
  }

  function onGripUp() {
    if (!dragging) return;
    dragging = false;
    const dismiss = dragOffset > DISMISS_DISTANCE;
    dragOffset = 0;
    if (dismiss) emojiPicker.close();
  }

  function pick(emoji: string) {
    emojiPicker.pick(emoji);
  }

  function close() {
    emojiPicker.close();
  }
</script>

{#if emojiPicker.isOpen && emojiPicker.anchor}
  {#if viewport.isPhone}
    <div class="scrim" role="presentation" use:portal onclick={dismissFromBackdrop}></div>
    <div
      class="sheet"
      role="dialog"
      aria-modal="true"
      aria-label="Emoji picker"
      use:portal
      style:transform={dragOffset > 0 ? `translateY(${dragOffset}px)` : undefined}
    >
      <div
        class="grip-area"
        role="presentation"
        onpointerdown={onGripDown}
        onpointermove={onGripMove}
        onpointerup={onGripUp}
        onpointercancel={onGripUp}
      >
        <span class="grip" aria-hidden="true"></span>
      </div>
      <div class="content">
        <EmojiPicker onpick={pick} onclose={close} />
      </div>
    </div>
  {:else if popoverPos}
    <div class="backdrop" role="presentation" use:portal onclick={dismissFromBackdrop}></div>
    <div
      class="popover"
      role="dialog"
      aria-label="Emoji picker"
      use:portal
      style:left="{popoverPos.x}px"
      style:top="{popoverPos.y}px"
    >
      <EmojiPicker onpick={pick} onclose={close} autofocus />
    </div>
  {/if}
{/if}

<style>
  /* Desktop: a transparent click-catcher under a floating popover. */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: transparent;
  }

  .popover {
    position: fixed;
    z-index: 101;
    box-sizing: border-box;
    width: 360px;
    height: min(420px, calc(100vh - 16px));
    overflow: hidden;
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    box-shadow: var(--alloy-elevation-lg);
  }

  /* Phones: dimmed scrim and a bottom sheet, below the context menu layers (109/110). */
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: var(--alloy-color-scrim);
  }

  .sheet {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 101;
    display: flex;
    flex-direction: column;
    height: 65vh;
    max-height: calc(100dvh - var(--alloy-safe-area-top));
    padding-bottom: var(--alloy-safe-area-bottom);
    border-radius: var(--alloy-radii-lg) var(--alloy-radii-lg) 0 0;
    background: var(--alloy-color-surface-elevated);
    box-shadow: var(--alloy-elevation-lg);
    overflow: hidden;
    transition: transform var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }

  .grip-area {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    height: 28px;
    touch-action: none;
    cursor: grab;
  }

  .grip {
    width: 40px;
    height: 4px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-border-prominent);
  }

  .content {
    flex: 1 1 auto;
    min-height: 0;
  }
</style>
