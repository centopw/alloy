<script lang="ts">
  import { untrack } from 'svelte';
  import { Avatar, Badge, Select } from '../common';
  import ConfirmDialog from '../common/ConfirmDialog.svelte';
  import TagChip from '../tags/TagChip.svelte';
  import TagEditDialog from '../tags/TagEditDialog.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Check, CheckCheck, Compass, Copy, ExternalLink, ImagePlus, Info, Link, LogOut, Lock, Pencil, Plus, Search, StickyNote, X, BellOff } from '$lib/icons';
  import { client } from '$lib/ipc';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { pickFile } from '$lib/platform/files';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { expect } from '$lib/stores/errors';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { roomNotify } from '$lib/stores/roomNotify.svelte';
  import { displayTags, TAG_COLOUR_VARS } from '$lib/stores/tags';
  import { tags } from '$lib/stores/tags.svelte';
  import { groupRoomsByTag, ROOM_SORTS, type RoomSort } from '$lib/stores/roomSort';
  import type { NotificationLevel } from '$lib/generated';
  import { ui } from '$lib/stores/ui.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { formatListTime } from '$lib/content/format';
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
  let newTagRoomId = $state<string | null>(null);
  const SORT_LABELS: Record<RoomSort, string> = {
    activity: 'Recent activity',
    name: 'Name',
    unread: 'Unread first',
    tag: 'By tag',
  };
  const sortOptions = ROOM_SORTS.map((value) => ({ value, label: SORT_LABELS[value] }));

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
    roomsStore.markRead(roomId);
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

  /** One level choice of a room's Notifications group; the level in effect is marked "(current)". */
  function notifyItem(r: RoomItem, label: string, target: NotificationLevel | null): MenuItem {
    const isDefault = roomNotify.isDefault(r.id);
    const current = target === null ? isDefault === true : isDefault === false && roomNotify.levelOf(r.id) === target;
    return {
      id: `notify-${target ?? 'default'}`,
      label: current ? `${label} (current)` : label,
      disabled: roomNotify.saving(r.id),
      onselect: () => void roomNotify.set(r.id, target),
    };
  }

  function roomItems(r: RoomItem): MenuItem[] {
    void roomInfo.load(r.id);
    void roomNotify.load(r.id);
    const d = roomInfo.details(r.id);
    const unread = roomsStore.unreadOf(r);
    return [
      { id: 'open', label: 'Open', icon: ExternalLink, onselect: () => openRoom(r.id) },
      { id: 'mark-read', label: 'Mark as read', icon: CheckCheck, disabled: unread === 0, onselect: () => void markRead(r.id) },
      { separator: true },
      notifyItem(r, 'Use default', null),
      notifyItem(r, 'All messages', 'All'),
      notifyItem(r, 'Mentions and keywords only', 'Mentions'),
      notifyItem(r, 'Mute', 'Mute'),
      { separator: true },
      ...tags.defs.map((d): MenuItem => ({
        id: `tag-${d.id}`,
        label: d.name,
        selected: r.tags.includes(d.id),
        onselect: () => void tags.setForRoom(r.id, d.id, !r.tags.includes(d.id)),
      })),
      { id: 'new-tag', label: 'New tag...', icon: Plus, onselect: () => (newTagRoomId = r.id) },
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
  const joined = $derived(roomsStore.visible.filter((r) => r.membership !== 'Invited'));
  // With sections, direct messages get their own People section when roomListPeople is on; otherwise they sit with Rooms.
  const rooms = $derived(joined.filter((r) => !prefs.roomListPeople || !r.is_direct));
  const direct = $derived(prefs.roomListPeople ? joined.filter((r) => r.is_direct) : []);
  // Without sections: invites first, then every joined room in the engine's activity order.
  const flat = $derived([...invites, ...joined]);
  const nothing = $derived(roomsStore.visible.length === 0);
  const tagDefs = $derived(displayTags(tags.defs, roomsStore.rooms));
  const tagById = $derived(new Map(tagDefs.map((d) => [d.id, d] as const)));
  const tagChips = $derived(tagDefs.map((def) => ({ def, count: roomsStore.rooms.filter((r) => r.tags.includes(def.id)).length })));

  // A filter on a tag that no longer exists (deleted here or elsewhere) would hide every room.
  $effect(() => {
    const filter = roomsStore.tagFilter;
    if (filter !== null && !tagById.has(filter)) roomsStore.tagFilter = null;
  });

  // The muted icon and the menu's current choice read these levels, so load them for the visible joined rooms.
  $effect(() => {
    const ids = roomsStore.visible.filter((r) => r.membership === 'Joined').map((r) => r.id);
    untrack(() => {
      for (const id of ids) void roomNotify.load(id);
    });
  });
  const leaveMessage = $derived(
    pendingLeave ? `You will leave ${pendingLeave.title}. You need a new invitation to join again.` : '',
  );
</script>

{#snippet roomRow(r: RoomItem)}
  {@const unread = roomsStore.unreadOf(r)}
  {@const active = r.id === roomsStore.activeRoomId}
  {@const muted = roomNotify.isMuted(r.id)}
  <button
    type="button"
    class="row"
    class:active
    class:unread={unread > 0 && !muted}
    class:muted
    aria-current={active ? 'true' : undefined}
    use:contextmenu={() => roomItems(r)}
    onclick={() => openRoom(r.id)}
  >
    <span class="fixed">
      <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size={viewport.isPhone ? 48 : 'sm'} shape={r.is_direct ? 'circle' : 'rounded'} />
    </span>
    <span class="text">
      <span class="title">{roomTitle(r)}</span>
      {#if prefs.roomListPreviews && r.latest_event}
        <span class="preview">{r.latest_event.body}</span>
      {/if}
    </span>
    <span class="end">
      {#if viewport.isPhone && r.latest_event}
        <time class="time" datetime={new Date(r.latest_event.timestamp_millis).toISOString()}>{formatListTime(r.latest_event.timestamp_millis)}</time>
      {/if}
      <span class="marks">
        {#if r.tags.length > 0}
          <span class="tag-dots" role="img" aria-label={`Tags: ${r.tags.map((id) => tagById.get(id)?.name ?? id).join(', ')}`}>
            {#each r.tags.slice(0, 3) as id (id)}
              <span class="tag-dot" style:background={TAG_COLOUR_VARS[tagById.get(id)?.colour ?? 'grey']}></span>
            {/each}
          </span>
        {/if}
        {#if r.encrypted}
          <span class="fixed"><Icon icon={Lock} size={16} label="Encrypted" /></span>
        {/if}
        {#if muted}
          <span class="fixed"><Icon icon={BellOff} size={16} label="Notifications muted" /></span>
        {/if}
        {#if unread > 0}
          {#if muted}
            <span class="fixed"><span class="dot" aria-hidden="true"></span></span>
          {:else}
            <span class="fixed"><Badge count={unread} /></span>
          {/if}
        {/if}
      </span>
    </span>
  </button>
{/snippet}

{#snippet inviteRow(r: RoomItem)}
  <div class="row invite" use:contextmenu={() => inviteItems(r)}>
    <span class="fixed">
      <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size={viewport.isPhone ? 48 : 'sm'} shape={r.is_direct ? 'circle' : 'rounded'} />
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
{/snippet}

{#snippet listRow(r: RoomItem)}
  {#if r.membership === 'Invited'}
    {@render inviteRow(r)}
  {:else}
    {@render roomRow(r)}
  {/if}
{/snippet}

{#snippet roomList(list: RoomItem[])}
  {#if prefs.roomSort === 'tag'}
    {#each groupRoomsByTag(list, tagDefs) as group (group.tag?.id ?? '')}
      <section class="tag-group" aria-label={group.tag?.name ?? 'Untagged'}>
        <h4>{group.tag?.name ?? 'Untagged'}</h4>
        <ul>
          {#each group.rooms as r (r.id)}
            <li>{@render listRow(r)}</li>
          {/each}
        </ul>
      </section>
    {/each}
  {:else}
    <ul>
      {#each list as r (r.id)}
        <li>{@render listRow(r)}</li>
      {/each}
    </ul>
  {/if}
{/snippet}

<section class="sidebar" aria-label="Rooms">
  <header class="head">
    <h2 class="scope" title={scopeTitle}>{viewport.isPhone ? 'Chats' : scopeTitle}</h2>
    {#if !viewport.isPhone}
      <button type="button" class="icon" aria-label="Browse public rooms" onclick={() => onbrowse?.()}>
        <Icon icon={Compass} size={20} />
      </button>
    {/if}
    <button type="button" class="icon" aria-label="New room" onclick={() => onnew?.()}>
      <Icon icon={Plus} size={20} />
    </button>
  </header>

  <div class="search">
    <Icon icon={Search} size={16} />
    <input type="search" placeholder="Search rooms" aria-label="Search rooms" bind:value={roomsStore.query} />
    <kbd>{shortcutLabel('K')}</kbd>
  </div>

  {#if tagChips.length > 0}
    <div class="tag-filter" role="toolbar" aria-label="Filter rooms by tag">
      <TagChip label="All" pressed={roomsStore.tagFilter === null} onclick={() => (roomsStore.tagFilter = null)} />
      {#each tagChips as chip (chip.def.id)}
        <TagChip
          label={chip.def.name}
          colour={chip.def.colour}
          count={chip.count}
          pressed={roomsStore.tagFilter === chip.def.id}
          onclick={() => (roomsStore.tagFilter = roomsStore.tagFilter === chip.def.id ? null : chip.def.id)}
        />
      {/each}
    </div>
  {/if}

  <div class="controls">
    <Select id="room-sort" class="sort-select" label="Sort rooms" value={prefs.roomSort} options={sortOptions} onchange={(v) => prefs.set('roomSort', v as RoomSort)} />
    {#if tags.error}
      <p class="error" role="alert">{tags.error}</p>
    {/if}
  </div>

  <div class="groups">
    {#if prefs.roomListSections}
      {#if invites.length > 0}
        <section class="group" aria-label="Invites">
          <h3>Invites</h3>
          <ul>
            {#each invites as r (r.id)}
              <li>{@render inviteRow(r)}</li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if rooms.length > 0}
        <section class="group" aria-label="Rooms">
          <h3>Rooms</h3>
          {@render roomList(rooms)}
        </section>
      {/if}

      {#if direct.length > 0}
        <section class="group" aria-label="People">
          <h3>People</h3>
          {@render roomList(direct)}
        </section>
      {/if}
    {:else if flat.length > 0}
      <section class="group" aria-label="Room list">
        {@render roomList(flat)}
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

<TagEditDialog
  open={newTagRoomId !== null}
  tag={null}
  onclose={() => (newTagRoomId = null)}
  onsaved={(created) => {
    if (newTagRoomId) void tags.setForRoom(newTagRoomId, created.id, true);
  }}
/>

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
  .tag-filter {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-1);
    margin: 0 var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
  }
  .controls {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-1);
    margin: 0 var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
  }
  /* The sort control is secondary: a quiet text-style trigger, not a boxed field. */
  .controls :global(.select-trigger.sort-select) {
    min-height: 28px;
    padding-inline: var(--alloy-layout-row-pad-x);
    border-color: transparent;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .controls :global(.select-trigger.sort-select:hover) {
    background: var(--alloy-color-surface-hover);
  }
  .error {
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-status-error);
  }
  .groups {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
  }
  .group h3,
  .group h4 {
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
  .tag-dots {
    display: inline-flex;
    gap: var(--alloy-spacing-scale-1);
  }
  .tag-dot {
    width: var(--alloy-spacing-scale-2);
    height: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
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
  .text > .title {
    flex: none;
  }
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .row.muted {
    color: var(--alloy-color-text-muted);
  }
  .dot {
    width: var(--alloy-spacing-scale-2);
    height: var(--alloy-spacing-scale-2);
    margin: 0 var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-text-muted);
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
  /* Trailing cluster (time, unread, marks) flattens into the row on desktop, so desktop layout is unchanged. */
  .end,
  .marks {
    display: contents;
  }
  @media (max-width: 899px) {
    .head {
      height: 56px;
      border-bottom: 0;
    }
    .scope {
      font-size: var(--alloy-typography-size-xl);
    }
    .icon {
      width: 44px;
      height: 44px;
    }
    .search {
      min-height: 44px;
      box-sizing: border-box;
      margin: var(--alloy-spacing-scale-2) var(--alloy-layout-gutter);
      padding: 0 var(--alloy-layout-row-pad-x);
    }
    .search input {
      min-height: 44px;
      font-size: var(--alloy-typography-size-lg);
    }
    .tag-filter,
    .controls {
      margin-left: var(--alloy-layout-gutter);
      margin-right: var(--alloy-layout-gutter);
    }
    .controls :global(.select-trigger.sort-select) {
      min-height: 44px;
    }
    .groups {
      padding: 0 calc(var(--alloy-layout-gutter) - var(--alloy-layout-row-pad-x)) var(--alloy-spacing-scale-2);
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
    }
    .group h3 {
      display: flex;
      align-items: center;
      box-sizing: border-box;
      height: 40px;
      padding: 0 var(--alloy-layout-row-pad-x);
    }
    button.row {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr) auto;
      align-items: center;
      gap: var(--alloy-spacing-scale-3);
      min-height: 64px;
      box-sizing: border-box;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    button.row:active {
      background: var(--alloy-color-surface-hover);
    }
    .end {
      display: flex;
      flex-direction: column;
      align-items: flex-end;
      justify-content: space-between;
      align-self: stretch;
      gap: var(--alloy-spacing-scale-1);
    }
    .marks {
      display: flex;
      align-items: center;
      gap: var(--alloy-spacing-scale-1);
      min-height: 20px;
    }
    .marks :global(.alloy-badge) {
      min-width: 20px !important;
      height: 20px !important;
    }
    .time {
      font-size: var(--alloy-typography-size-xs);
      color: var(--alloy-color-text-muted);
      white-space: nowrap;
    }
    .row.unread .time {
      color: var(--alloy-color-accent-primary);
    }
    .preview {
      font-size: var(--alloy-typography-size-sm);
      color: var(--alloy-color-text-secondary);
    }
    .invite-actions {
      margin-top: var(--alloy-spacing-scale-2);
    }
    .invite-actions button {
      flex: 1;
      min-height: 44px;
      padding: 0 var(--alloy-layout-row-pad-x);
      border-radius: var(--alloy-radii-md);
      font-size: var(--alloy-typography-size-base);
    }
  }
</style>
