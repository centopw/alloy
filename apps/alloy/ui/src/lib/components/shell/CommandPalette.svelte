<script lang="ts">
  import { tick } from 'svelte';
  import { Avatar } from '../common';
  import { viewport } from '$lib/stores/viewport.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronLeft, Search } from '$lib/icons';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { pushBack } from '$lib/platform/backstack';
  import { handleModalTabKey } from '$lib/platform/focusTrap';

  import { t } from '$lib/i18n';
  interface Props {
    open: boolean;
    onclose: () => void;
    onpick: (roomId: string) => void;
  }
  let { open, onclose, onpick }: Props = $props();

  // Android Back closes the palette.
  $effect(() => {
    if (!open) return;
    return pushBack(() => onclose());
  });

  let query = $state('');
  let index = $state(0);
  let input: HTMLInputElement | undefined = $state();
  let paletteEl: HTMLDivElement | undefined = $state();
  /** The element that opened the palette, so Escape/backdrop/back can return focus to it. */
  let opener: HTMLElement | null = null;

  /**
   * Closes the palette and returns focus to the opener (A11Y-15). The run callback runs after the
   * focus move so follow-up surfaces capture the opener as their own return target instead of the
   * input that is about to be removed.
   */
  function closeAndRestore(then?: () => void) {
    const target = opener;
    opener = null;
    onclose();
    if (target?.isConnected) target.focus();
    then?.();
  }

  /** After a chat is chosen the row is the keyboard user's landing spot; phones show the chat
   * instead and the shell moves focus there, so a hidden row is left alone. */
  function focusChosenRow(roomId: string) {
    void tick().then(() => {
      const row = document.querySelector<HTMLElement>(
        `.row[data-room-id="${CSS.escape(roomId)}"]`
      );
      if (row && row.getClientRects().length > 0) row.focus();
    });
  }

  const results = $derived(
    roomsStore.rooms
      .filter((r) => roomTitle(r).toLowerCase().includes(query.trim().toLowerCase()))
      .slice(0, 8)
  );

  $effect(() => {
    if (open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      query = '';
      index = 0;
      void tick().then(() => input?.focus());
    }
  });

  $effect(() => {
    void query;
    index = 0;
  });

  // The highlighted option's id. The input keeps DOM focus and points at it with
  // aria-activedescendant, which is the combobox pattern: screen readers announce the active
  // option without moving focus off the text field.
  const activeOptionId = $derived(results[index] ? `palette-option-${results[index].id}` : undefined);

  /** Opens one chat and leaves the keyboard user on its row in the chat list. */
  function choose(roomId: string) {
    onpick(roomId);
    focusChosenRow(roomId);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') closeAndRestore();
    else if (e.key === 'ArrowDown') {
      e.preventDefault();
      index = Math.min(index + 1, results.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      index = Math.max(index - 1, 0);
    } else if (e.key === 'Enter' && results[index]) {
      e.preventDefault();
      choose(results[index].id);
    } else if (e.key === 'Tab') {
      // Options are not in the tab order (the input moves through them with aria-activedescendant),
      // so the only focusable elements are the back button and the input. The shared trap keeps
      // focus inside this aria-modal dialog and skips the display:none desktop back button.
      if (handleModalTabKey(e, paletteEl, document.activeElement)) e.preventDefault();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => closeAndRestore()}>
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label={t('command-palette-quick-switcher')}
      tabindex="-1"
      bind:this={paletteEl}
      onkeydown={onKey}
      onclick={(e) => e.stopPropagation()}
    >
      <div class="bar">
        <button type="button" class="back" aria-label={t('command-palette-close-quick-switcher')} onclick={() => closeAndRestore()}>
          <Icon icon={ChevronLeft} size={24} />
        </button>
        <span class="search-icon" aria-hidden="true"><Icon icon={Search} size={20} /></span>
        <input
          bind:this={input}
          bind:value={query}
          placeholder={t('command-palette-search-chats')}
          aria-label={t('command-palette-search-chats')}
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-results"
          aria-activedescendant={activeOptionId}
          aria-autocomplete="list"
        />
      </div>
      <ul id="palette-results" role="listbox">
        {#each results as r, i (r.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id={`palette-option-${r.id}`}
            role="option"
            aria-selected={i === index}
            class="option"
            class:sel={i === index}
            onclick={() => choose(r.id)}
          >
            <Avatar userId={r.id} displayName={roomTitle(r)} size={viewport.isPhone ? 48 : 'sm'} />
            <span>{roomTitle(r)}</span>
          </li>
        {/each}
        {#if results.length === 0}
          <li class="none">{t('command-palette-no-matching-conversations')}</li>
        {/if}
      </ul>
      <footer class="hints" aria-hidden="true">
        <span><kbd>↑</kbd><kbd>↓</kbd> {t('command-palette-navigate')}</span>
        <span><kbd>↵</kbd> {t('command-palette-open')}</span>
        <span><kbd>{t('command-palette-esc')}</kbd> {t('common-close')}</span>
      </footer>
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
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    z-index: 50;
  }
  .palette {
    width: min(520px, 92vw);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-shell);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
    overflow: hidden;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding-left: var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-glass-rim);
  }
  .search-icon {
    display: inline-flex;
    flex: none;
    color: var(--alloy-color-text-muted);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-lg);
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter) 0 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: var(--alloy-layout-row-inset);
  }
  /* A listbox option is presentational content: ARIA options must not contain buttons (A11Y-21),
     so the row itself is the click target and the input owns keyboard selection. */
  .option {
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    align-items: center;
    width: 100%;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border-radius: var(--alloy-radii-lg);
    cursor: pointer;
    text-align: left;
  }
  .option.sel,
  .option:hover {
    background: var(--alloy-glass-control-hover);
  }
  .none {
    padding: var(--alloy-spacing-scale-4);
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
  /* Keyboard affordances belong to the palette, not to the input placeholder. */
  .hints {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-4);
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-gutter);
    border-top: 1px solid var(--alloy-glass-rim);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }
  .hints span {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
  }
  .hints kbd {
    display: inline-grid;
    place-items: center;
    min-width: 18px;
    height: 18px;
    padding: 0 var(--alloy-spacing-scale-1);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-typography-size-xs);
    line-height: 1;
  }
  .back {
    display: none;
  }
  @media (max-width: 899px) {
    .backdrop {
      padding: 0;
      align-items: stretch;
      background: var(--alloy-canvas-art);
    }
    .palette {
      display: flex;
      flex-direction: column;
      width: 100%;
      height: 100%;
      border: 0;
      border-radius: 0;
      box-shadow: none;
    }
    .bar {
      display: flex;
      align-items: center;
      flex-shrink: 0;
      padding-top: var(--alloy-safe-area-top);
    }
    .hints {
      display: none;
    }
    .bar input {
      flex: 1;
      min-width: 0;
      padding: 0 var(--alloy-layout-gutter) 0 var(--alloy-spacing-scale-1);
    }
    .back {
      display: grid;
      place-items: center;
      flex: none;
      width: 44px;
      height: 44px;
      min-height: 44px;
      margin-left: var(--alloy-spacing-scale-1);
      padding: 0;
      border: 0;
      border-radius: var(--alloy-radii-full);
      background: transparent;
      color: var(--alloy-color-text-secondary);
      cursor: pointer;
    }
    ul {
      flex: 1;
      min-height: 0;
      overflow-y: auto;
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
      padding: var(--alloy-layout-row-inset) var(--alloy-layout-row-inset) calc(var(--alloy-safe-area-bottom) + var(--alloy-layout-row-inset));
    }
    .option {
      min-height: 64px;
      box-sizing: border-box;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
      font-size: var(--alloy-typography-size-base);
    }
  }
</style>
