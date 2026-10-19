<script lang="ts">
  import { untrack } from 'svelte';
  import { Icon, Search, X, ChevronLeft, Copy, CornerUpLeft, User } from '$lib/icons';
  import Avatar from '../common/Avatar.svelte';
  import UserName from '../common/UserName.svelte';
  import TextField from '../common/TextField.svelte';
  import Select from '../common/Select.svelte';
  import { formatDay, formatTime } from '$lib/content/format';
  import { displayNameFor } from '$lib/platform/names';
  import { copyText } from '$lib/platform/clipboard';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { highlightRuns } from '$lib/content/searchHighlight';
  import { roomSearch, type SearchResult } from '$lib/stores/roomSearch.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';

  interface Props {
    roomId: string;
    title: string;
    onclose: () => void;
  }

  let { roomId, title, onclose }: Props = $props();

  let inputEl: HTMLInputElement | null = $state(null);
  let listEl: HTMLUListElement | undefined = $state();
  /** Event ID of the hit being jumped to, so its row can show a busy state. */
  let jumpingId: string | null = $state(null);
  /** Set when the last jump could not reach the event (for example, it was redacted or pruned). */
  let missing = $state(false);

  // Bind the search to this room and keep member names available for the sender filter.
  $effect(() => {
    const id = roomId;
    untrack(() => {
      roomSearch.bind(id);
      void membersStore.loadMembers(id);
    });
    return () => roomSearch.stop();
  });

  $effect(() => {
    inputEl?.focus();
  });

  const senderOptions = $derived<{ value: string; label: string }[]>([
    { value: '', label: 'Anyone' },
    ...membersStore.membersOf(roomId).map((m) => ({
      value: m.user_id,
      label: displayNameFor(m.user_id, null, m.display_name ?? null)
    }))
  ]);

  const showIdle = $derived(roomSearch.mode === 'idle');
  const showEmpty = $derived(
    !showIdle && !roomSearch.loading && !roomSearch.error && roomSearch.results.length === 0
  );

  async function jump(hit: SearchResult) {
    missing = false;
    jumpingId = hit.event_id;
    const found = await timelineStore.jumpToEvent(hit.event_id);
    jumpingId = null;
    if (!found) {
      missing = true;
      return;
    }
    // On phones the panel covers the chat, so close it to show the message.
    if (viewport.isPhone) onclose();
  }

  function hitMenu(hit: SearchResult) {
    return () => [
      { id: 'jump', label: 'Jump to message', icon: CornerUpLeft, onselect: () => void jump(hit) },
      { id: 'copy', label: 'Copy text', icon: Copy, onselect: () => void copyText(hit.body) },
      { id: 'user', label: 'Copy user ID', icon: User, onselect: () => void copyText(hit.sender) }
    ];
  }

  /** Arrow keys move focus between result rows; ArrowDown from the search field enters the list. */
  function onListKeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const rows = Array.from(listEl?.querySelectorAll<HTMLButtonElement>('button.hit') ?? []);
    if (rows.length === 0) return;
    const at = rows.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === 'ArrowDown' ? Math.min(at + 1, rows.length - 1) : Math.max(at - 1, 0);
    e.preventDefault();
    rows[next === -1 ? 0 : next]?.focus();
  }

  function onInputKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      const first = listEl?.querySelector<HTMLButtonElement>('button.hit');
      if (first) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  function onpanelKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    }
  }

  function timeLabel(ms: number): string {
    return `${formatDay(ms)} ${formatTime(ms)}`;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside class="panel" aria-label="Search in room" onkeydown={onpanelKeydown}>
  <header class="bar">
    <h2 class="title">Search in {title}</h2>
    <button
      type="button"
      class="icon-btn"
      aria-label={viewport.isPhone ? 'Back' : 'Close search'}
      title={viewport.isPhone ? 'Back' : 'Close search'}
      onclick={onclose}
    >
      <Icon icon={viewport.isPhone ? ChevronLeft : X} size={20} />
    </button>
  </header>

  <div class="controls">
    <TextField
      bind:el={inputEl}
      class="query"
      type="search"
      placeholder="Search messages"
      aria-label="Search messages"
      icon={Search}
      value={roomSearch.query}
      oninput={(e) => roomSearch.setQuery(e.currentTarget.value)}
      onkeydown={onInputKeydown}
    />
    <Select
      label="From"
      value={roomSearch.sender ?? ''}
      options={senderOptions}
      onchange={(v) => roomSearch.setSender(v || null)}
    />
  </div>

  <div class="body">
    {#if showIdle}
      <p class="hint">Type to search messages in this room, or choose a sender.</p>
    {/if}

    {#if roomSearch.mode === 'local' && !showIdle}
      <p class="hint" role="note">Searching messages loaded on this device.</p>
    {/if}

    {#if roomSearch.loading}
      <p class="hint" role="status">Searching...</p>
    {/if}

    {#if roomSearch.error}
      <div class="notice" role="alert">
        <p>{roomSearch.error}</p>
        <button type="button" class="action" onclick={() => roomSearch.setQuery(roomSearch.query)}>
          Try again
        </button>
      </div>
    {/if}

    {#if missing}
      <p class="notice" role="alert">That message is no longer available.</p>
    {/if}

    {#if showEmpty}
      <p class="hint">No messages found.</p>
    {/if}

    {#if roomSearch.results.length > 0}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul class="results" bind:this={listEl} onkeydown={onListKeydown} aria-label="Search results">
        {#each roomSearch.results as hit (hit.event_id)}
          <li>
            <button
              type="button"
              class="hit"
              disabled={jumpingId !== null}
              onclick={() => void jump(hit)}
              use:contextmenu={hitMenu(hit)}
            >
              <span class="meta">
                <Avatar userId={hit.sender} displayName={hit.sender_display_name} size="sm" />
                <UserName userId={hit.sender} displayName={hit.sender_display_name} roomId={roomId} />
                <time class="time" datetime={new Date(hit.timestamp_millis).toISOString()}>
                  {timeLabel(hit.timestamp_millis)}
                </time>
              </span>
              <span class="snippet">
                {#each highlightRuns(hit.body, roomSearch.query) as run, i (i)}{#if run.match}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}
              </span>
            </button>
          </li>
        {/each}
      </ul>

      {#if roomSearch.capped}
        <p class="hint">Showing the first 200 matches.</p>
      {/if}

      {#if roomSearch.canLoadMore}
        <button type="button" class="action wide" disabled={roomSearch.loading} onclick={() => roomSearch.loadMore()}>
          Load more results
        </button>
      {/if}
    {/if}

    {#if roomSearch.canSearchOlder}
      <button type="button" class="action wide" disabled={roomSearch.loading} onclick={() => roomSearch.searchOlder()}>
        Search older messages
      </button>
    {/if}
  </div>
</aside>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--alloy-color-surface-panel);
    color: var(--alloy-color-text-primary);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    flex-shrink: 0;
  }

  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  .controls {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    flex-shrink: 0;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
  }

  .hint {
    margin: 0;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
  }

  .notice {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-sm);
  }

  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }

  .hit {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    width: 100%;
    min-height: 44px;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
    overflow: hidden;
  }

  .hit:hover,
  .hit:focus-visible {
    background: var(--alloy-color-surface-hover);
  }

  .hit:disabled {
    cursor: progress;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }

  .time {
    flex-shrink: 0;
    margin-inline-start: auto;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }

  .snippet {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }

  mark {
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-text-primary);
    border-radius: var(--alloy-radii-sm);
  }

  .action {
    align-self: flex-start;
    min-height: 44px;
    padding: 0 var(--alloy-spacing-scale-3);
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-accent-primary);
    font: inherit;
    cursor: pointer;
  }

  .action.wide {
    align-self: stretch;
  }

  .action:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  @media (max-width: 899px) {
    .icon-btn {
      width: 44px;
      height: 44px;
    }
  }
</style>
