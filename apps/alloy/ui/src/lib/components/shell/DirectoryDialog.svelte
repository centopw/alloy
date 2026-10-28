<script lang="ts">
  import { untrack } from 'svelte';
  import { Avatar, Button, Modal, TextField } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Search, Users } from '$lib/icons';
  import { client } from '$lib/ipc';
  import type { PublicRoomDto } from '$lib/generated/index.js';
  import { directoryStore } from '$lib/stores/directory.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import { roomsStore } from '$lib/stores/rooms.svelte';
  import { session } from '$lib/stores/session.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
    onjoined: (roomId: string) => void;
  }
  let { open, onclose, onjoined }: Props = $props();

  /** Delay before a typed query is sent, so each keystroke does not hit the server. */
  const SEARCH_DEBOUNCE_MS = 250;

  let query = $state('');
  let joining = $state<string | null>(null);
  let joinError = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!open) return;
    const account = session.account;
    if (!account) return;
    untrack(() => {
      query = '';
      joinError = null;
      void directoryStore.search(account.id, '');
    });
  });

  $effect(() => () => clearTimeout(timer));

  function onInput(value: string) {
    query = value;
    clearTimeout(timer);
    timer = setTimeout(() => {
      const account = session.account;
      if (account) void directoryStore.search(account.id, value);
    }, SEARCH_DEBOUNCE_MS);
  }

  function loadMore() {
    const account = session.account;
    if (account) void directoryStore.loadMore(account.id);
  }

  async function join(room: PublicRoomDto) {
    joinError = null;
    joining = room.room_id;
    try {
      const res = await client.joinRoom(room.alias ?? room.room_id);
      if (res.type !== 'RoomJoined') throw new Error('unexpected response');
      roomsStore.select(res.payload.room_id);
      onjoined(res.payload.room_id);
      onclose();
    } catch (err) {
      joinError = errorMessage(err);
    } finally {
      joining = null;
    }
  }
</script>

<Modal {open} title="Browse public rooms" {onclose} width="480px">
  <div class="body">
    <div class="search-wrap">
      <TextField
        type="search"
        icon={Search}
        aria-label="Search public rooms"
        placeholder="Search public rooms"
        autocomplete="off"
        value={query}
        oninput={(e) => onInput(e.currentTarget.value)}
      />
    </div>

    {#if directoryStore.error}
      <p class="error" role="alert">{directoryStore.error}</p>
    {/if}
    {#if joinError}
      <p class="error" role="alert">{joinError}</p>
    {/if}

    {#if directoryStore.loading && directoryStore.rooms.length === 0}
      <p class="status" role="status">Searching...</p>
    {:else if !directoryStore.loading && directoryStore.rooms.length === 0 && !directoryStore.error}
      <p class="status">No public rooms match.</p>
    {/if}

    <ul class="results">
      {#each directoryStore.rooms as room (room.room_id)}
        {@const title = room.name ?? room.alias ?? room.room_id}
        <li>
          <Avatar userId={room.room_id} displayName={title} src={room.avatar_url} size={viewport.isPhone ? 48 : 'md'} shape="rounded" />
          <div class="meta">
            <span class="name" title={title}>{title}</span>
            {#if room.alias}<span class="alias" title={room.alias}>{room.alias}</span>{/if}
            {#if room.topic}<span class="topic" title={room.topic}>{room.topic}</span>{/if}
            <span class="members">
              <Icon icon={Users} size={16} />
              {room.num_joined_members} {room.num_joined_members === 1 ? 'member' : 'members'}
            </span>
          </div>
          <div class="action">
            {#if room.joined}
              <Button variant="ghost" disabled ariaLabel="Joined {title}">Joined</Button>
            {:else}
              <Button
                disabled={joining !== null}
                ariaLabel="Join {title}"
                onclick={() => join(room)}
              >
                {joining === room.room_id ? 'Joining...' : 'Join'}
              </Button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>

    {#if directoryStore.hasMore}
      <div class="more">
        <Button variant="ghost" disabled={directoryStore.loading} onclick={loadMore}>
          {directoryStore.loading ? 'Loading...' : 'Load more'}
        </Button>
      </div>
    {/if}
  </div>
</Modal>

<style>
  /* Modal's content area already pads 16px; min-height/overflow let this body scroll inside it. */
  .body {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: auto;
  }
  .search-wrap {
    margin-top: var(--alloy-spacing-scale-4);
  }
  .results {
    list-style: none;
    margin: var(--alloy-spacing-scale-3) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    max-height: 420px;
    overflow-y: auto;
  }
  .results li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-md);
  }
  .results li:hover {
    background: var(--alloy-color-surface-hover);
  }
  .meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name,
  .alias {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    color: var(--alloy-color-text-primary);
    font-weight: 600;
  }
  .alias,
  .members {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .members {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .topic {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-muted);
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .action {
    flex: none;
    min-width: 84px;
    display: flex;
    justify-content: flex-end;
  }
  .action :global(.alloy-button) {
    flex: none;
    white-space: nowrap;
  }
  .status {
    margin: var(--alloy-spacing-scale-3) 0 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .error {
    margin: var(--alloy-spacing-scale-3) 0 0;
    color: var(--alloy-color-status-error);
  }
  .more {
    display: flex;
    justify-content: center;
    margin-top: var(--alloy-spacing-scale-3);
  }
  @media (max-width: 899px) {
    .body {
      overflow: visible;
    }
    .search-wrap {
      position: sticky;
      top: 0;
      z-index: 1;
      margin: 0 calc(-1 * var(--alloy-layout-gutter));
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-gutter);
      border: 0;
      border-radius: 0;
      background: var(--alloy-glass-fill-strong);
      -webkit-backdrop-filter: var(--alloy-glass-filter);
      backdrop-filter: var(--alloy-glass-filter);
    }
    .results {
      max-height: none;
      overflow: visible;
    }
    .results li {
      min-height: 64px;
      box-sizing: border-box;
    }
    .name {
      font-size: var(--alloy-typography-size-base);
    }
    .action :global(.alloy-button),
    .more :global(.alloy-button) {
      min-height: 44px;
    }
  }
</style>
