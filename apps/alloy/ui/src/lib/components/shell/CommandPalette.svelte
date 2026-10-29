<script lang="ts">
  import { tick } from 'svelte';
  import { Avatar } from '../common';
  import { viewport } from '$lib/stores/viewport.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronLeft, Search } from '$lib/icons';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { pushBack } from '$lib/platform/backstack';
  import { client } from '$lib/ipc';
  import { expect } from '$lib/stores/errors';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import type { UserSearchResultDto } from '$lib/generated';

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

  const results = $derived(
    roomsStore.rooms
      .filter((r) => r.membership === 'Joined' && !r.is_space)
      .filter((r) => roomTitle(r).toLowerCase().includes(query.trim().toLowerCase()))
      .slice(0, 8)
  );

  // People come from the server's user directory: the same query, a page of six. The directory is
  // the only way to reach someone you share no room with, which is why this lives in the palette.
  const PEOPLE_LIMIT = 6;
  const PEOPLE_MIN_CHARS = 2;
  let people = $state<UserSearchResultDto[]>([]);
  let peopleBusy = $state(false);
  let peopleError = $state<string | null>(null);
  let peopleRequest = 0;

  $effect(() => {
    const q = query.trim();
    const id = ++peopleRequest;
    if (q.length < PEOPLE_MIN_CHARS) {
      people = [];
      peopleBusy = false;
      peopleError = null;
      return;
    }
    peopleBusy = true;
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const res = await client.invoke({ action: 'SearchUsers', params: { query: q, limit: PEOPLE_LIMIT } });
          if (id !== peopleRequest) return;
          people = expect(res, 'UserSearchResults').payload.results;
          peopleError = null;
        } catch (e) {
          if (id !== peopleRequest) return;
          people = [];
          peopleError = e instanceof Error ? e.message : 'User search failed';
        } finally {
          if (id === peopleRequest) peopleBusy = false;
        }
      })();
    }, 250);
    return () => window.clearTimeout(timer);
  });

  /** Rooms first, then people: one list for the keyboard, two groups on screen. */
  const options = $derived([
    ...results.map((r) => ({ kind: 'room' as const, id: r.id, title: roomTitle(r) })),
    ...people.map((p) => ({ kind: 'person' as const, id: `person-${p.user_id}`, title: p.display_name ?? p.user_id, user: p })),
  ]);

  $effect(() => {
    if (open) {
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
  const activeOptionId = $derived(options[index] ? `palette-option-${options[index].id}` : undefined);

  /** Opens a room, or the profile card of a person found in the directory. */
  function choose(option: (typeof options)[number]) {
    if (option.kind === 'room') {
      onpick(option.id);
      return;
    }
    const anchor = input?.getBoundingClientRect();
    onclose();
    profileCard.open(option.user.user_id, null, { x: anchor?.left ?? 0, y: anchor?.bottom ?? 0 });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
    else if (e.key === 'ArrowDown') {
      e.preventDefault();
      index = Math.min(index + 1, options.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      index = Math.max(index - 1, 0);
    } else if (e.key === 'Enter' && options[index]) {
      e.preventDefault();
      choose(options[index]);
    } else if (e.key === 'Tab') {
      // Trap Tab inside the modal. Options are not in the tab order (keyboard moves through the
      // input), so the only focusable elements are the back button and the input.
      // Filter to visible elements: the back button is display:none on desktop, so focusing it
      // would silently fail and let focus escape the modal.
      const focusables = Array.from(
        paletteEl?.querySelectorAll<HTMLElement>('input, button:not([tabindex="-1"])') ?? []
      ).filter((el) => el.getClientRects().length > 0);
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
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
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="Quick switcher"
      tabindex="-1"
      bind:this={paletteEl}
      onkeydown={onKey}
      onclick={(e) => e.stopPropagation()}
    >
      <div class="bar">
        <button type="button" class="back" aria-label="Close quick switcher" onclick={onclose}>
          <Icon icon={ChevronLeft} size={24} />
        </button>
        <span class="search-icon" aria-hidden="true"><Icon icon={Search} size={20} /></span>
        <input
          bind:this={input}
          bind:value={query}
          placeholder="Search rooms"
          aria-label="Search rooms"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-results"
          aria-activedescendant={activeOptionId}
          aria-autocomplete="list"
        />
      </div>
      <ul id="palette-results" role="listbox">
        {#each results as r, i (r.id)}
          <li id={`palette-option-${r.id}`} role="option" aria-selected={i === index}>
            <button type="button" tabindex="-1" class:sel={i === index} onclick={() => choose(options[i])}>
              <Avatar userId={r.id} displayName={roomTitle(r)} size={viewport.isPhone ? 48 : 'sm'} />
              <span>{roomTitle(r)}</span>
            </button>
          </li>
        {/each}
        {#if people.length > 0}
          <li class="group" role="presentation">People</li>
          {#each people as p, j (p.user_id)}
            <li id={`palette-option-person-${p.user_id}`} role="option" aria-selected={results.length + j === index}>
              <button
                type="button"
                tabindex="-1"
                class:sel={results.length + j === index}
                onclick={() => choose(options[results.length + j])}
              >
                <Avatar userId={p.user_id} displayName={p.display_name} src={p.avatar_url} size={viewport.isPhone ? 48 : 'sm'} />
                <span class="who">
                  <span class="name">{p.display_name ?? p.user_id}</span>
                  <span class="id">{p.user_id}{p.already_in_room ? ' · in a room' : ''}</span>
                </span>
              </button>
            </li>
          {/each}
        {:else if results.length === 0 && !peopleBusy}
          <li class="none">No matching conversations or people</li>
        {/if}
        {#if peopleError}
          <li class="none" role="alert">{peopleError}</li>
        {/if}
      </ul>
      <footer class="hints" aria-hidden="true">
        <span><kbd>↑</kbd><kbd>↓</kbd> Navigate</span>
        <span><kbd>↵</kbd> Open</span>
        <span><kbd>esc</kbd> Close</span>
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
  button {
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    align-items: center;
    width: 100%;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border-radius: var(--alloy-radii-lg);
    cursor: pointer;
    text-align: left;
  }
  button.sel,
  button:hover {
    background: var(--alloy-glass-control-hover);
  }
  .group {
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x) var(--alloy-spacing-scale-1);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .who .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .who .id {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
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
    li button {
      min-height: 64px;
      box-sizing: border-box;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
      font-size: var(--alloy-typography-size-base);
    }
  }
</style>
