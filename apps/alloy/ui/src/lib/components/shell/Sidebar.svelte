<script lang="ts">
  import { Avatar, Badge } from '../common';
  import ConfirmDialog from '../common/ConfirmDialog.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Check, CheckCheck, Compass, Copy, ExternalLink, ImagePlus, Info, Link, LogOut, Lock, Pencil, Plus, Search, StickyNote, X } from '$lib/icons';
  import { client } from '$lib/ipc';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { pickFile } from '$lib/platform/files';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { expect } from '$lib/stores/errors';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import RenameRoomDialog from './RenameRoomDialog.svelte';
  import UpdateBanner from './UpdateBanner.svelte';
  import UserPanel from './UserPanel.svelte';

  interface Props {
    onselect?: (roomId: string) => void;
    onnew?: () => void;
    onbrowse?: () => void;
    onsettings?: () => void;
  }
  let { onselect, onnew, onbrowse, onsettings }: Props = $props();

  type RoomItem = (typeof roomsStore.rooms)[number];

  let inviteBusy = $state<string | null>(null);
  let renameRoomId = $state<string | null>(null);
  let pendingLeave = $state<{ id: string; title: string } | null>(null);

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

  function openRoom(roomId: string) {
    roomsStore.select(roomId);
    onselect?.(roomId);
  }

  function showRoomInfo(roomId: string) {
    openRoom(roomId);
    ui.openRoomInfo('info');
  }

  async function markRead(roomId: string) {
    try {
      await client.invoke({ action: 'MarkRoomRead', params: { room_id: roomId } });
    } catch {
      /* the room list stream reflects the real state; nothing to roll back */
    }
  }

  async function changePicture(roomId: string) {
    try {
      const path = await pickFile();
      if (!path) return;
      await roomInfo.setAvatar(roomId, path);
      // Failures are kept on the room's info state; the details panel shows them.
      if (roomInfo.error(roomId)) showRoomInfo(roomId);
    } catch {
      showRoomInfo(roomId);
    }
  }

  function copyText(text: string) {
    void navigator.clipboard?.writeText(text).catch(() => undefined);
  }

  async function leaveRoom(roomId: string) {
    expect(await client.invoke({ action: 'LeaveRoom', params: { room_id: roomId } }), 'RoomLeft');
    if (roomsStore.activeRoomId === roomId) roomsStore.select(null);
  }

  async function confirmLeave() {
    if (pendingLeave) await leaveRoom(pendingLeave.id);
  }

  function roomItems(r: RoomItem): MenuItem[] {
    void roomInfo.load(r.id);
    const d = roomInfo.details(r.id);
    const unread = r.unread_notifications.notification_count;
    return [
      { id: 'open', label: 'Open', icon: ExternalLink, onselect: () => openRoom(r.id) },
      { id: 'mark-read', label: 'Mark as read', icon: CheckCheck, disabled: unread === 0, onselect: () => void markRead(r.id) },
      { separator: true },
      { id: 'details', label: 'Room details', icon: Info, onselect: () => showRoomInfo(r.id) },
      { id: 'rename', label: 'Rename room', icon: Pencil, disabled: d !== null && !d.can_set_name, onselect: () => (renameRoomId = r.id) },
      {
        id: 'picture',
        label: 'Change room picture',
        icon: ImagePlus,
        disabled: d !== null && !d.can_set_avatar,
        onselect: () => void changePicture(r.id),
      },
      { id: 'note', label: 'Edit note', icon: StickyNote, onselect: () => showRoomInfo(r.id) },
      { separator: true },
      { id: 'copy-link', label: 'Copy room link', icon: Link, onselect: () => copyText(`https://matrix.to/#/${r.id}`) },
      { id: 'copy-id', label: 'Copy room ID', icon: Copy, onselect: () => copyText(r.id) },
      { separator: true },
      { id: 'leave', label: 'Leave room', icon: LogOut, danger: true, onselect: () => (pendingLeave = { id: r.id, title: roomTitle(r) }) },
    ];
  }

  function inviteItems(r: RoomItem): MenuItem[] {
    const busy = inviteBusy === r.id;
    return [
      { id: 'accept', label: 'Accept invitation', icon: Check, disabled: busy, onselect: () => void answerInvite(r.id, true) },
      { id: 'decline', label: 'Decline invitation', icon: X, disabled: busy, onselect: () => void answerInvite(r.id, false) },
    ];
  }

  const scopeSpace = $derived(ui.scope === 'home' ? null : (roomsStore.rooms.find((r) => r.id === ui.scope) ?? null));
  const scopeTitle = $derived(scopeSpace ? roomTitle(scopeSpace) : 'Home');

  const invites = $derived(roomsStore.visible.filter((r) => r.membership === 'Invited'));
  const rooms = $derived(roomsStore.visible.filter((r) => r.membership !== 'Invited' && !r.is_direct));
  const direct = $derived(roomsStore.visible.filter((r) => r.membership !== 'Invited' && r.is_direct));
  const nothing = $derived(invites.length + rooms.length + direct.length === 0);
  const leaveMessage = $derived(
    pendingLeave ? `You will leave ${pendingLeave.title}. You need a new invitation to join again.` : '',
  );
</script>

{#snippet roomRow(r: RoomItem)}
  {@const unread = r.unread_notifications.notification_count}
  {@const active = r.id === roomsStore.activeRoomId}
  <button
    type="button"
    class="row"
    class:active
    class:unread={unread > 0}
    aria-current={active ? 'true' : undefined}
    use:contextmenu={() => roomItems(r)}
    onclick={() => openRoom(r.id)}
  >
    <span class="fixed">
      <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size="sm" shape={r.is_direct ? 'circle' : 'rounded'} />
    </span>
    <span class="title">{roomTitle(r)}</span>
    {#if r.encrypted}
      <span class="fixed"><Icon icon={Lock} size={16} label="Encrypted" /></span>
    {/if}
    {#if unread > 0}
      <span class="fixed"><Badge count={unread} /></span>
    {/if}
  </button>
{/snippet}

<section class="sidebar" aria-label="Rooms">
  <header class="head">
    <h2 class="scope" title={scopeTitle}>{scopeTitle}</h2>
    <button type="button" class="icon" aria-label="Browse public rooms" onclick={() => onbrowse?.()}>
      <Icon icon={Compass} size={20} />
    </button>
    <button type="button" class="icon" aria-label="New room" onclick={() => onnew?.()}>
      <Icon icon={Plus} size={20} />
    </button>
  </header>

  <div class="search">
    <Icon icon={Search} size={16} />
    <input type="search" placeholder="Search rooms" aria-label="Search rooms" bind:value={roomsStore.query} />
    <kbd>{shortcutLabel('K')}</kbd>
  </div>

  <div class="groups">
    {#if invites.length > 0}
      <section class="group" aria-label="Invites">
        <h3>Invites</h3>
        <ul>
          {#each invites as r (r.id)}
            <li>
              <div class="row invite" use:contextmenu={() => inviteItems(r)}>
                <span class="fixed">
                  <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size="sm" shape={r.is_direct ? 'circle' : 'rounded'} />
                </span>
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

  <UpdateBanner />
  <UserPanel onsettings={() => onsettings?.()} />
</section>

{#if renameRoomId}
  <RenameRoomDialog roomId={renameRoomId} open onclose={() => (renameRoomId = null)} />
{/if}

<ConfirmDialog
  open={pendingLeave !== null}
  title="Leave room"
  message={leaveMessage}
  confirmLabel="Leave"
  danger
  onconfirm={confirmLeave}
  onclose={() => (pendingLeave = null)}
/>

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
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
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
    margin: var(--alloy-spacing-scale-3) var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-1) var(--alloy-layout-row-pad-x);
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
    padding: 0 var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
  }
  .group h3 {
    margin: 0;
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x) var(--alloy-spacing-scale-1);
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
  .fixed {
    flex: none;
    display: inline-flex;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-width: 0;
    padding: var(--alloy-spacing-scale-1) var(--alloy-layout-row-pad-x);
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
    margin: var(--alloy-layout-section-gap) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-muted);
    text-align: center;
  }
  button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
