<script lang="ts">
  import { tick } from 'svelte';
  import type { GlobalSearchHit } from '$lib/generated';
  import { formatListTime } from '$lib/content/format';
  import { highlightRuns } from '$lib/content/searchHighlight';
  import { Avatar } from '../common';
  import { viewport } from '$lib/stores/viewport.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronLeft, Search } from '$lib/icons';
  import { globalSearch, isSearchable } from '$lib/stores/globalSearch.svelte';
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

  const trimmedQuery = $derived(query.trim());
  const results = $derived(
    roomsStore.rooms
      .filter((r) => roomTitle(r).toLowerCase().includes(trimmedQuery.toLowerCase()))
      .slice(0, 8)
  );
  const hitResults = $derived(trimmedQuery.length >= 2 ? globalSearch.hits.slice(0, 12) : []);
  const showMessageSection = $derived(
    trimmedQuery.length >= 2 &&
      (isSearchable(trimmedQuery) ||
        hitResults.length > 0 ||
        globalSearch.loading ||
        globalSearch.error !== null)
  );
  const totalOptions = $derived(results.length + hitResults.length);

  $effect(() => {
    if (open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      query = '';
      index = 0;
      void tick().then(() => input?.focus());
    }
  });

  $effect(() => {
    if (!open) return;
    if (trimmedQuery.length >= 2) {
      globalSearch.schedule(query);
    } else {
      globalSearch.reset();
    }
  });

  $effect(() => {
    if (!open) return;
    return () => {
      globalSearch.reset();
    };
  });

  $effect(() => {
    void query;
    index = 0;
  });

  $effect(() => {
    if (totalOptions === 0) {
      index = 0;
    } else if (index >= totalOptions) {
      index = totalOptions - 1;
    }
  });

  // The highlighted option's id. The input keeps DOM focus and points at it with
  // aria-activedescendant, which is the combobox pattern: screen readers announce the active
  // option without moving focus off the text field.
  const activeOptionId = $derived.by(() => {
    if (index < results.length) {
      const room = results[index];
      return room ? `palette-option-${room.id}` : undefined;
    }
    const hitIndex = index - results.length;
    return hitResults[hitIndex] ? `palette-option-hit-${hitIndex}` : undefined;
  });

  /** Opens one chat and leaves the keyboard user on its row in the chat list. */
  function choose(roomId: string) {
    onpick(roomId);
    focusChosenRow(roomId);
  }

  /** Opens the hit's chat and jumps to the matching message once its timeline is ready. */
  function chooseHit(hit: GlobalSearchHit) {
    void globalSearch.open(hit, onpick);
  }

  function scrollActiveOptionIntoView() {
    void tick().then(() => {
      if (activeOptionId) {
        document.getElementById(activeOptionId)?.scrollIntoView?.({ block: 'nearest' });
      }
    });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') closeAndRestore();
    else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (totalOptions > 0) {
        index = Math.min(index + 1, totalOptions - 1);
        scrollActiveOptionIntoView();
      }
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (totalOptions > 0) {
        index = Math.max(index - 1, 0);
        scrollActiveOptionIntoView();
      }
    } else if (e.key === 'Enter' && totalOptions > 0) {
      e.preventDefault();
      if (index < results.length) {
        choose(results[index].id);
      } else {
        const hit = hitResults[index - results.length];
        if (hit) chooseHit(hit);
      }
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
        {#if showMessageSection && results.length > 0}
          <li class="section-head" role="presentation">{t('sidebar-chats')}</li>
        {/if}
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
        {#if showMessageSection}
          <li class="section-head" role="presentation">
            <span>{t('sidebar-messages')}</span>
            <span class="section-count">{globalSearch.hits.length}</span>
          </li>
          {#if globalSearch.error}
            <li class="status error" role="alert">{globalSearch.error}</li>
          {:else if globalSearch.loading && hitResults.length === 0}
            <li class="status" role="status">{t('sidebar-searching')}</li>
          {:else if hitResults.length === 0}
            <li class="status" role="presentation">{t('sidebar-no-messages-found')}</li>
          {:else}
            {#each hitResults as hit, j (hit.room_id + hit.event_id)}
              {@const optionIndex = results.length + j}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <li
                id={`palette-option-hit-${j}`}
                role="option"
                aria-selected={optionIndex === index}
                class="option hit"
                class:sel={optionIndex === index}
                onclick={() => chooseHit(hit)}
              >
                <span class="hit-meta">
                  <Avatar userId={hit.sender} displayName={hit.sender_display_name} size="sm" />
                  <span class="hit-room">
                    <span>{hit.room_name ?? t('sidebar-room')}</span>
                    <time datetime={new Date(hit.timestamp_millis).toISOString()}>{formatListTime(hit.timestamp_millis)}</time>
                  </span>
                </span>
                <span class="hit-body">
                  <span class="hit-sender">{hit.sender_display_name ?? hit.sender}:</span>
                  {#each highlightRuns(hit.body, globalSearch.query) as run, k (k)}{#if run.match}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}
                </span>
              </li>
            {/each}
          {/if}
        {:else if results.length === 0}
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
    padding-top: 12dvh;
    z-index: 50;
  }
  .palette {
    display: flex;
    flex-direction: column;
    width: min(520px, 92vw);
    max-height: 82dvh;
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
    flex-shrink: 0;
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
    flex: 1;
    min-height: 0;
    list-style: none;
    margin: 0;
    padding: var(--alloy-layout-row-inset);
    max-height: min(440px, 60dvh);
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-y;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x) var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }
  .section-count {
    padding: 1px var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-control);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: 0;
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-secondary);
  }
  .status {
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .status.error {
    color: var(--alloy-color-status-error);
  }
  /* A listbox option is presentational content: ARIA options must not contain buttons (A11Y-21),
     so the row itself is the click target and the input owns keyboard selection. */
  .option {
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    align-items: center;
    box-sizing: border-box;
    width: 100%;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border-radius: var(--alloy-radii-lg);
    cursor: pointer;
    text-align: left;
  }
  .option.hit {
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
  }
  .hit-meta {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }
  .hit-room {
    display: flex;
    flex: 1;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .hit-room > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hit-room time {
    flex: none;
    font-weight: var(--alloy-typography-weight-regular);
    font-size: var(--alloy-font-size-meta);
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-muted);
  }
  .hit-body {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: var(--alloy-font-size-preview);
    line-height: 1.4;
    color: var(--alloy-color-text-secondary);
  }
  .hit-sender {
    color: var(--alloy-color-text-primary);
  }
  .option.hit mark {
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
  }
  .option.sel,
  .option:active {
    background: var(--alloy-glass-control-hover);
  }
  @media (hover: hover) {
    .option:hover {
      background: var(--alloy-glass-control-hover);
    }
  }
  .none {
    padding: var(--alloy-spacing-scale-4);
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
  /* Keyboard affordances belong to the palette, not to the input placeholder. */
  .hints {
    display: flex;
    flex-shrink: 0;
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
  @media (hover: hover) {
    .back:hover {
      background: var(--alloy-glass-control);
      color: var(--alloy-color-text-primary);
    }
  }
  .back:active {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  .back:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  @media (max-width: 899px), (pointer: coarse) {
    .option {
      min-height: var(--alloy-touch-target, 44px);
    }
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
      height: 100dvh;
      max-height: 100dvh;
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
      width: var(--alloy-touch-target, 44px);
      height: var(--alloy-touch-target, 44px);
      min-height: var(--alloy-touch-target, 44px);
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
      max-height: none;
      overflow-y: auto;
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
      touch-action: pan-y;
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
