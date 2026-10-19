<script lang="ts">
  import { Avatar, Button, Modal, TextField } from '../common';
  import { Search } from '$lib/icons';
  import { forwardStore } from '$lib/stores/forward.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';

  let query = $state('');

  $effect(() => {
    if (forwardStore.open) query = '';
  });

  // Rooms you can send to, most recently active first.
  const targets = $derived(
    roomsStore.rooms
      .filter((r) => r.membership === 'Joined' && !r.is_space)
      .filter((r) => !query.trim() || roomTitle(r).toLowerCase().includes(query.trim().toLowerCase()))
      .toSorted((a, b) => (b.latest_event?.timestamp_millis ?? 0) - (a.latest_event?.timestamp_millis ?? 0)),
  );

  const count = $derived(forwardStore.eventIds.length);
</script>

<!-- A pressed Send button turns disabled and drops focus, so Escape is also read at the window. -->
<svelte:window onkeydown={(e) => forwardStore.open && e.key === 'Escape' && forwardStore.close()} />

<Modal
  open={forwardStore.open}
  title={count > 1 ? `Forward ${count} messages` : 'Forward message'}
  onclose={() => forwardStore.close()}
  width="440px"
>
  <TextField bind:value={query} icon={Search} placeholder="Search rooms" aria-label="Search rooms" autocomplete="off" />
  <ul class="rooms" aria-label="Rooms">
    {#each targets as room (room.id)}
      {@const state = forwardStore.results[room.id]}
      <li class="room">
        <Avatar userId={room.id} displayName={roomTitle(room)} src={room.avatar_url} size={36} shape={room.is_direct ? 'circle' : 'rounded'} />
        <span class="name">
          {roomTitle(room)}
          {#if typeof state === 'object'}<span class="err" role="alert">{state.error}</span>{/if}
        </span>
        <Button
          variant={state === 'sent' ? 'ghost' : 'primary'}
          size="sm"
          disabled={state === 'sending' || state === 'sent'}
          onclick={() => void forwardStore.sendTo(room.id)}
        >
          {state === 'sent' ? 'Sent' : state === 'sending' ? 'Sending' : typeof state === 'object' ? 'Retry' : 'Send'}
        </Button>
      </li>
    {:else}
      <li class="empty">No matching rooms.</li>
    {/each}
  </ul>
  <div class="footer">
    <Button variant="ghost" onclick={() => forwardStore.close()}>Done</Button>
  </div>
</Modal>

<style>
  .rooms {
    max-height: 50dvh;
    margin: var(--alloy-spacing-scale-3) 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .room {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    min-height: 52px;
  }
  .name {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .err {
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-xs);
    white-space: normal;
  }
  .empty {
    padding: var(--alloy-spacing-scale-4);
    color: var(--alloy-color-text-muted);
    text-align: center;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
  }
</style>
