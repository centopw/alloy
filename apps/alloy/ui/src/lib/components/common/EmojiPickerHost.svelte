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
  let searchFocused = $state(false);
  let searching = $state(false);

  let returnFocus: HTMLElement | null = null;

  const popoverPos = $derived.by(() => {
    const anchor = emojiPicker.anchor;
    if (!anchor || viewport.isPhone) return null;
    return placeAnchor(anchor, POPOVER, {
      width: window.innerWidth,
      height: window.innerHeight
    });
  });

  const isExpanded = $derived(viewport.isPhone && (searchFocused || searching));

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
    // Only desktop popovers close on resize; on phones the soft keyboard resizes the viewport.
    if (!emojiPicker.isOpen || viewport.isPhone) return;
    const onResize = () => emojiPicker.close();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });

  $effect(() => {
    if (!emojiPicker.isOpen) {
      searchFocused = false;
      searching = false;
    }
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
    searchFocused = false;
    searching = false;
    emojiPicker.close();
  }
</script>

{#if emojiPicker.isOpen && emojiPicker.anchor}
  {#if viewport.isPhone}
    <div class="scrim" role="presentation" use:portal onclick={dismissFromBackdrop}></div>
    <div
      class="sheet"
      class:expanded={isExpanded}
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
        <EmojiPicker
          onpick={pick}
          onclose={close}
          onsearchfocus={(f) => (searchFocused = f)}
          onsearching={(s) => (searching = s)}
        />
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
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-panel);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
  }

  /* Phones: dimmed scrim and a bottom sheet, below the context menu layers (109/110). */
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: var(--alloy-color-scrim);
    -webkit-backdrop-filter: blur(10px) saturate(140%);
    backdrop-filter: blur(10px) saturate(140%);
  }

  .sheet {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 101;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    height: 65dvh;
    max-height: calc(100dvh - var(--alloy-safe-area-top) - var(--alloy-spacing-scale-2));
    padding-bottom: var(--alloy-safe-area-bottom);
    border: 1px solid var(--alloy-glass-rim);
    border-bottom: 0;
    border-radius: var(--alloy-glass-radius-panel) var(--alloy-glass-radius-panel) 0 0;
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
    overflow: hidden;
    transition: transform var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      height var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }

  .sheet.expanded {
    height: calc(100dvh - var(--alloy-safe-area-top) - var(--alloy-spacing-scale-2));
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
    display: flex;
    flex-direction: column;
    flex: 1 1 0;
    min-height: 0;
  }
</style>
