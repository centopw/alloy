<script lang="ts">
  import { Avatar, Badge } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Compass, Lock, Plus, Search } from '$lib/icons';
  import { client } from '$lib/ipc';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import UserPanel from './UserPanel.svelte';

  interface Props {
    onselect?: (roomId: string) => void;
    onnew?: () => void;
    onbrowse?: () => void;
    onsettings?: () => void;
  }
  let { onselect, onnew, onbrowse, onsettings }: Props = $props();

  let inviteBusy = $state<string | null>(null);

  async function answerInvite(roomId: string, accept: boolean) {
    inviteBusy = roomId;
    try {
      if (accept) {
        await client.joinRoom(roomId);
        roomsStore.select(roomId);
        onselect?.(roomId);
      } else {
        await client.leaveRoom(roomId);
      }
    } catch {
      /* the room list stream reflects the real state; nothing to roll back */
    } finally {
      inviteBusy = null;
    }
  }

  const scopeSpace = $derived(ui.scope === 'home' ? null : (roomsStore.rooms.find((r) => r.id === ui.scope) ?? null));
  const scopeTitle = $derived(scopeSpace ? roomTitle(scopeSpace) : 'Home');

  const invites = $derived(roomsStore.visible.filter((r) => r.membership === 'Invited'));
  const rooms = $derived(roomsStore.visible.filter((r) => r.membership !== 'Invited' && !r.is_direct));
  const direct = $derived(roomsStore.visible.filter((r) => r.membership !== 'Invited' && r.is_direct));
  const nothing = $derived(invites.length + rooms.length + direct.length === 0);
</script>

{#snippet roomRow(r: (typeof roomsStore.rooms)[number])}
  {@const unread = r.unread_notifications.notification_count}
  {@const active = r.id === roomsStore.activeRoomId}
  <button
    type="button"
    class="row"
    class:active
    class:unread={unread > 0}
    aria-current={active ? 'true' : undefined}
    onclick={() => {
      roomsStore.select(r.id);
      onselect?.(r.id);
    }}
  >
    <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size="sm" shape={r.is_direct ? 'circle' : 'rounded'} />
    <span class="title">{roomTitle(r)}</span>
    {#if r.encrypted}
      <Icon icon={Lock} size={16} label="Encrypted" />
    {/if}
    <Badge count={unread} />
  </button>
{/snippet}

<section class="sidebar" aria-label="Rooms">
  <header class="head">
    <h2 class="scope" title={scopeTitle}>{scopeTitle}</h2>
    <button type="button" class="icon" aria-label="Browse public rooms" title="Browse public rooms" onclick={() => onbrowse?.()}>
      <Icon icon={Compass} size={20} />
    </button>
    <button type="button" class="icon" aria-label="New room" title="New room" onclick={() => onnew?.()}>
      <Icon icon={Plus} size={20} />
    </button>
  </header>

  <div class="search">
    <Icon icon={Search} size={16} />
    <input type="search" placeholder="Search rooms" aria-label="Search rooms" bind:value={roomsStore.query} />
    <kbd>Ctrl K</kbd>
  </div>

  <div class="groups">
    {#if invites.length > 0}
      <section class="group" aria-label="Invites">
        <h3>Invites</h3>
        <ul>
          {#each invites as r (r.id)}
            <li>
              <div class="row invite">
                <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size="sm" shape={r.is_direct ? 'circle' : 'rounded'} />
                <div class="text">
                  <span class="title">{roomTitle(r)}</span>
                  <span class="sub">Invitation</span>
                  <div class="invite-actions">
                    <button type="button" disabled={inviteBusy === r.id} onclick={() => answerInvite(r.id, true)}>Accept</button>
                    <button type="button" class="decline" disabled={inviteBusy === r.id} onclick={() => answerInvite(r.id, false)}>Decline</button>
                  </div>
                </div>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if rooms.length > 0}
      <section class="group" aria-label="Rooms">
        <h3>Rooms</h3>
        <ul>
          {#each rooms as r (r.id)}
            <li>{@render roomRow(r)}</li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if direct.length > 0}
      <section class="group" aria-label="Direct messages">
        <h3>Direct messages</h3>
        <ul>
          {#each direct as r (r.id)}
            <li>{@render roomRow(r)}</li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if nothing}
      <p class="empty">
        {#if !roomsStore.loaded}Loading conversations{:else if roomsStore.query}No matches{:else}Nothing here yet{/if}
      </p>
    {/if}
  </div>

  <UserPanel onsettings={() => onsettings?.()} />
</section>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    height: 100%;
    background: var(--alloy-color-surface-panel);
    color: var(--alloy-color-text-secondary);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    height: 48px;
    padding: 0 var(--alloy-spacing-scale-3) 0 var(--alloy-spacing-scale-4);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  .scope {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    margin: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-muted);
  }
  .search:focus-within {
    outline: 2px solid var(--alloy-color-accent-primary);
  }
  .search input {
    flex: 1;
    min-width: 0;
    padding: var(--alloy-spacing-scale-1) 0;
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font: inherit;
  }
  .search input:focus {
    outline: none;
  }
  @media (max-width: 899px) {
    .search kbd {
      display: none;
    }
  }
  .search kbd {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .groups {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
  }
  .group h3 {
    margin: 0;
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }
  .group ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-width: 0;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  button.row {
    cursor: pointer;
  }
  button.row:hover {
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-primary);
  }
  button.row.active {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .row.unread {
    color: var(--alloy-color-text-primary);
  }
  .row.unread .title {
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-base);
  }
  .row.invite {
    align-items: flex-start;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    flex: 1;
    min-width: 0;
  }
  .sub {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .invite-actions {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-1);
  }
  .invite-actions button {
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-3);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .invite-actions button.decline {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .invite-actions button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .icon:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .empty {
    margin: var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-muted);
    text-align: center;
  }
  button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
