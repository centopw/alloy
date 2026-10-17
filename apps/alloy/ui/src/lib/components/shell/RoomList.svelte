<script lang="ts">
  import { Avatar, Badge, SegmentedControl } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Search } from '$lib/icons';
  import { roomsStore, roomTitle, type RoomFilter } from '$lib/stores/rooms.svelte';
  import { formatListTime } from '$lib/content/format';
  import { session } from '$lib/stores/session.svelte';

  interface Props {
    onselect?: (roomId: string) => void;
    onsearchfocus?: () => void;
  }
  let { onselect, onsearchfocus }: Props = $props();

  const filters = [
    { value: 'all', label: 'All' },
    { value: 'unread', label: 'Unread' },
    { value: 'direct', label: 'Direct' },
    { value: 'rooms', label: 'Rooms' },
  ];

  function preview(r: (typeof roomsStore.rooms)[number]): string {
    const e = r.latest_event;
    if (!e) return r.encrypted ? 'Encrypted room' : 'No messages yet';
    const mine = e.sender === session.account?.user_id;
    const who = mine ? 'You' : r.is_direct ? '' : e.sender.split(':')[0].replace('@', '');
    return who ? `${who}: ${e.body}` : e.body;
  }
</script>

<section class="list" aria-label="Conversations">
  <div class="search">
    <Icon icon={Search} size={16} />
    <input
      type="search"
      placeholder="Search"
      aria-label="Search conversations"
      bind:value={roomsStore.query}
      onfocus={onsearchfocus}
    />
    <kbd>Ctrl K</kbd>
  </div>

  <div class="chips">
    <SegmentedControl
      options={filters}
      value={roomsStore.filter}
      label="Filter conversations"
      onchange={(v) => (roomsStore.filter = v as RoomFilter)}
    />
  </div>

  <ul class="rooms">
    {#each roomsStore.visible as r (r.id)}
      {@const unread = r.unread_notifications.notification_count}
      <li>
        <button
          type="button"
          class="room"
          class:active={r.id === roomsStore.activeRoomId}
          aria-current={r.id === roomsStore.activeRoomId ? 'true' : undefined}
          onclick={() => {
            roomsStore.select(r.id);
            onselect?.(r.id);
          }}
        >
          <Avatar userId={r.id} displayName={roomTitle(r)} size="lg" />
          <div class="text">
            <div class="top">
              <span class="title">{roomTitle(r)}</span>
              {#if r.latest_event}
                <span class="time">{formatListTime(r.latest_event.timestamp_millis)}</span>
              {/if}
            </div>
            <div class="bottom">
              <span class="preview">{preview(r)}</span>
              {#if unread > 0}<Badge count={unread} />{/if}
            </div>
          </div>
        </button>
      </li>
    {:else}
      <li class="empty">
        {#if !roomsStore.loaded}Loading conversations{:else if roomsStore.query}No matches{:else}Nothing here yet{/if}
      </li>
    {/each}
  </ul>
</section>

<style>
  .list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    background: var(--alloy-color-surface-panel);
    border-right: 1px solid var(--alloy-color-border-subtle);
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    margin: var(--alloy-spacing-scale-3);
    padding: 0 var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-full);
    color: var(--alloy-color-text-secondary);
  }
  .search:focus-within {
    outline: 2px solid var(--alloy-color-accent-primary);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font: inherit;
    padding: 8px 0;
  }
  kbd {
    font: inherit;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .chips {
    padding: 0 var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-2);
  }
  .rooms {
    list-style: none;
    margin: 0;
    padding: 0 var(--alloy-spacing-scale-2);
    overflow-y: auto;
    flex: 1;
  }
  .room {
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    align-items: center;
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    padding: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-md);
    cursor: pointer;
  }
  .room:hover {
    background: var(--alloy-color-surface-hover);
  }
  .room.active {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
  }
  .room.active .preview,
  .room.active .time {
    color: inherit;
    opacity: 0.85;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .top,
  .bottom {
    display: flex;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    align-items: center;
  }
  .title {
    font-weight: var(--alloy-typography-weight-semibold);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time,
  .preview {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    padding: var(--alloy-spacing-scale-6);
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
</style>
