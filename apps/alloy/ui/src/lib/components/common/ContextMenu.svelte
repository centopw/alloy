<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { portal } from '$lib/platform/portal';
  import { pushBack } from '$lib/platform/backstack';
  import { placeMenu } from '$lib/platform/contextmenu';
  import {
    contextMenu,
    isSeparator,
    type MenuAction,
    type MenuItem,
    type MenuPoint,
  } from '$lib/stores/contextmenu.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';

  const TYPEAHEAD_RESET_MS = 600;

  const CANCEL: MenuAction = { id: 'cancel', label: 'Cancel' };

  let menuEl: HTMLDivElement | undefined = $state();
  let scrimEl: HTMLDivElement | undefined = $state();
  let typed = '';
  let typedTimer: number | null = null;

  /** Position the menu at contextMenu.at, flipping and clamping to the viewport after each measure. */
  function place(node: HTMLElement, _deps: { at: MenuPoint; items: MenuItem[] }) {
    // Phone: the menu is a bottom sheet placed by CSS, so no coordinates are applied.
    if (viewport.isPhone) return;
    const apply = () => {
      const p = placeMenu(
        contextMenu.at,
        { width: node.offsetWidth, height: node.offsetHeight },
        { width: window.innerWidth, height: window.innerHeight },
      );
      node.style.left = `${p.x}px`;
      node.style.top = `${p.y}px`;
    };
    apply();
    return { update: apply };
  }

  function enabledItems(): HTMLElement[] {
    if (!menuEl) return [];
    return Array.from(
      menuEl.querySelectorAll<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"])'),
    );
  }

  function focusStep(current: HTMLElement, delta: 1 | -1) {
    const els = enabledItems();
    if (els.length === 0) return;
    const i = els.indexOf(current);
    const next = i < 0 ? (delta > 0 ? 0 : els.length - 1) : (i + delta + els.length) % els.length;
    els[next].focus({ preventScroll: true });
  }

  function typeahead(current: HTMLElement, key: string) {
    if (typedTimer !== null) window.clearTimeout(typedTimer);
    typed += key.toLowerCase();
    typedTimer = window.setTimeout(() => {
      typed = '';
      typedTimer = null;
    }, TYPEAHEAD_RESET_MS);
    const els = enabledItems();
    const start = els.indexOf(current);
    for (let k = 1; k <= els.length; k++) {
      const el = els[(start + k) % els.length];
      if (el.textContent?.trim().toLowerCase().startsWith(typed)) {
        el.focus({ preventScroll: true });
        return;
      }
    }
  }

  /** Pointer activation: close first so the menu is gone before the action runs. */
  function selectByPointer(item: MenuAction) {
    if (item.disabled) return;
    contextMenu.close();
    item.onselect?.();
  }

  /** Keyboard activation: restore focus to the opener first, then run the action. */
  function selectByKeyboard(item: MenuAction) {
    if (item.disabled) return;
    contextMenu.dismiss();
    item.onselect?.();
  }

  function onItemKeydown(e: KeyboardEvent, item: MenuAction) {
    const el = e.currentTarget as HTMLElement;
    const els = enabledItems();
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        focusStep(el, 1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        focusStep(el, -1);
        break;
      case 'Home':
        e.preventDefault();
        els[0]?.focus({ preventScroll: true });
        break;
      case 'End':
        e.preventDefault();
        els[els.length - 1]?.focus({ preventScroll: true });
        break;
      case 'Enter':
      case ' ':
        e.preventDefault();
        selectByKeyboard(item);
        break;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
          e.preventDefault();
          typeahead(el, e.key);
        }
    }
  }

  function hoverItem(e: PointerEvent, item: MenuAction) {
    if (item.disabled) return;
    (e.currentTarget as HTMLElement).focus({ preventScroll: true });
  }

  // Focus the first enabled item whenever the menu opens or its items are replaced.
  $effect(() => {
    void contextMenu.items;
    if (!contextMenu.isOpen) return;
    tick().then(() => enabledItems()[0]?.focus({ preventScroll: true }));
  });

  // Keep the Android Back button closing the menu before leaving the app (phone layouts only).
  $effect(() => {
    if (!contextMenu.isOpen) return;
    return pushBack(() => contextMenu.close());
  });

  // Dismissal while open: Escape and Tab from the keyboard; outside press, scroll, blur, resize.
  $effect(() => {
    if (!contextMenu.isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        contextMenu.dismiss();
      } else if (e.key === 'Tab') {
        contextMenu.close();
      }
    };
    const onPointerDown = (e: PointerEvent) => {
      // The phone scrim closes on click, not pointerdown, so the tap cannot fall through to the content beneath.
      if (e.target === scrimEl) return;
      if (e.target instanceof Node && menuEl?.contains(e.target)) return;
      contextMenu.close();
    };
    const onScroll = (e: Event) => {
      if (e.target instanceof Node && menuEl?.contains(e.target)) return;
      contextMenu.close();
    };
    const onClose = () => contextMenu.close();
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('blur', onClose);
    window.addEventListener('resize', onClose);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('blur', onClose);
      window.removeEventListener('resize', onClose);
    };
  });

  $effect(() => () => {
    if (typedTimer !== null) window.clearTimeout(typedTimer);
  });
</script>

{#if contextMenu.isOpen}
  {#if viewport.isPhone}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="sheet-scrim" bind:this={scrimEl} use:portal onclick={() => contextMenu.close()}></div>
  {/if}
  <div
    bind:this={menuEl}
    class="context-menu"
    class:sheet={viewport.isPhone}
    role="menu"
    aria-label="Actions"
    use:portal
    use:place={{ at: contextMenu.at, items: contextMenu.items }}
  >
    {#each contextMenu.items as item, i (i)}
      {#if isSeparator(item)}
        <div class="separator" role="separator"></div>
      {:else}
        <div
          class="item"
          class:danger={item.danger}
          class:disabled={item.disabled}
          role="menuitem"
          tabindex="-1"
          aria-disabled={item.disabled ? 'true' : undefined}
          aria-keyshortcuts={item.shortcut}
          onclick={() => selectByPointer(item)}
          onpointermove={(e) => hoverItem(e, item)}
          onkeydown={(e) => onItemKeydown(e, item)}
        >
          {#if item.icon}
            <span class="icon"><Icon icon={item.icon} size={viewport.isPhone ? 20 : 16} /></span>
          {/if}
          <span class="label">{item.label}</span>
          {#if item.shortcut}
            <kbd class="shortcut">{item.shortcut}</kbd>
          {/if}
        </div>
      {/if}
    {/each}

    {#if viewport.isPhone}
      <div class="separator" role="separator"></div>
      <div
        class="item cancel"
        role="menuitem"
        tabindex="-1"
        onclick={() => selectByPointer(CANCEL)}
        onpointermove={(e) => hoverItem(e, CANCEL)}
        onkeydown={(e) => onItemKeydown(e, CANCEL)}
      >
        <span class="label">Cancel</span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .context-menu {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 110;
    min-width: 180px;
    max-width: 280px;
    padding: var(--alloy-spacing-scale-1);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-regular);
    line-height: var(--alloy-typography-line-height-normal);
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    height: 32px;
    padding: 0 var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-sm);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    outline: none;
  }

  .item:focus {
    background: var(--alloy-color-surface-hover);
  }

  .item.danger {
    color: var(--alloy-color-status-error);
  }

  .item.disabled {
    color: var(--alloy-color-text-muted);
    cursor: default;
  }

  .icon {
    display: inline-flex;
    flex: none;
  }

  .label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .shortcut {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }

  .separator {
    height: 1px;
    margin: var(--alloy-spacing-scale-1) 0;
    background: var(--alloy-color-border-subtle);
  }

  /* Phone: the menu is a bottom action sheet. Rows are 52px for touch; the Cancel row closes it. */
  .context-menu.sheet {
    top: auto;
    left: 0;
    right: 0;
    bottom: 0;
    min-width: 0;
    max-width: none;
    max-height: 90dvh;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2)
      calc(var(--alloy-spacing-scale-2) + var(--alloy-safe-area-bottom));
    border: 0;
    border-radius: var(--alloy-radii-lg) var(--alloy-radii-lg) 0 0;
    box-shadow: var(--alloy-elevation-lg);
    animation: alloy-action-sheet-up var(--alloy-motion-duration-normal) ease-out backwards;
  }
  .sheet .item {
    height: 52px;
    padding: 0 var(--alloy-spacing-scale-4);
    font-size: var(--alloy-typography-size-lg);
    border-radius: var(--alloy-radii-md);
  }
  .sheet .shortcut {
    display: none;
  }
  .sheet .item.cancel {
    justify-content: center;
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .sheet .item.cancel .label {
    flex: none;
  }

  /* Phone: scrim behind the action sheet. Tapping it closes the sheet (click, not pointerdown). */
  .sheet-scrim {
    position: fixed;
    inset: 0;
    z-index: 109;
    background: rgba(0, 0, 0, 0.5);
    animation: alloy-fade-in var(--alloy-motion-duration-normal) ease-out backwards;
  }

  @keyframes alloy-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes alloy-action-sheet-up {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
</style>
