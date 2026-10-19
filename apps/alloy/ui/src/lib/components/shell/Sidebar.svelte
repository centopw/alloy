<script lang="ts">
  import { untrack } from 'svelte';
  import { Avatar, Badge } from '../common';
  import ConfirmDialog from '../common/ConfirmDialog.svelte';
  import TagChip from '../tags/TagChip.svelte';
  import TagEditDialog from '../tags/TagEditDialog.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ArrowUpDown, Bell, BellOff, Check, CheckCheck, ChevronDown, ChevronUp, Compass, Copy, ImagePlus, Info, Link, LogOut, Lock, Palette, Pencil, Plus, Search, Settings, StickyNote, Tag, Trash2, X } from '$lib/icons';
  import { client } from '$lib/ipc';
  import { copyText } from '$lib/platform/clipboard';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { pickFile } from '$lib/platform/files';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import { contextMenu, type MenuItem } from '$lib/stores/contextmenu.svelte';
  import { expect } from '$lib/stores/errors';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { ROOM_FILTERS, roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { roomNotify } from '$lib/stores/roomNotify.svelte';
  import { displayTags, TAG_COLOUR_VARS, TAG_COLOURS } from '$lib/stores/tags';
  import { tags } from '$lib/stores/tags.svelte';
  import { groupRoomsByTag, ROOM_SORTS, type RoomSort } from '$lib/stores/roomSort';
  import type { NotificationLevel, TagColour } from '$lib/generated';
  import { ui } from '$lib/stores/ui.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { formatListTime } from '$lib/content/format';
  import RenameRoomDialog from './RenameRoomDialog.svelte';
  import UserPanel from './UserPanel.svelte';

  interface Props {
    onselect?: (roomId: string) => void;
    onnew?: () => void;
    onbrowse?: () => void;
    onsettings?: (section?: 'tags') => void;
  }
  let { onselect, onnew, onbrowse, onsettings }: Props = $props();

  type RoomItem = (typeof roomsStore.rooms)[number];

  let inviteBusy = $state<string | null>(null);
  let renameRoomId = $state<string | null>(null);
  let pendingLeave = $state<{ id: string; title: string } | null>(null);
  let renameTagId = $state<string | null>(null);
  let pendingTagDelete = $state<{ id: string; name: string; count: number } | null>(null);
  const SORT_LABELS: Record<RoomSort, string> = {
    activity: 'Activity',
    name: 'Name',
    unread: 'Unread first',
    tag: 'By tag',
  };

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

  async function leaveRoom(roomId: string) {
    expect(await client.invoke({ action: 'LeaveRoom', params: { room_id: roomId } }), 'RoomLeft');
    if (roomsStore.activeRoomId === roomId) roomsStore.select(null);
  }

  async function confirmLeave() {
    if (pendingLeave) await leaveRoom(pendingLeave.id);
  }

  /** One level choice of a room's Notifications submenu; the level in effect is the selected row. */
  function notifyItem(r: RoomItem, label: string, target: NotificationLevel | null): MenuItem {
    const isDefault = roomNotify.isDefault(r.id);
    const current = target === null ? isDefault === true : isDefault === false && roomNotify.levelOf(r.id) === target;
    return {
      id: `notify-${target ?? 'default'}`,
      label,
      selected: current,
      disabled: roomNotify.saving(r.id),
      onselect: () => void roomNotify.set(r.id, target),
    };
  }

  /** Tag ids of a room as they are now, so the Tags submenu reflects each toggle. */
  function liveTags(roomId: string): readonly string[] {
    return roomsStore.rooms.find((x) => x.id === roomId)?.tags ?? [];
  }

  async function createTagFor(roomId: string, name: string) {
    const created = await tags.create(name, 'blue');
    if (created) await tags.setForRoom(roomId, created.id, true);
  }

  function roomItems(r: RoomItem): MenuItem[] {
    void roomInfo.load(r.id);
    void roomNotify.load(r.id);
    const d = roomInfo.details(r.id);
    const unread = roomsStore.unreadOf(r);
    return [
      { id: 'mark-read', label: 'Mark as read', icon: CheckCheck, disabled: unread === 0, onselect: () => void markRead(r.id) },
      { separator: true },
      {
        id: 'notifications',
        label: 'Notifications',
        icon: Bell,
        submenu: {
          items: () => [
            notifyItem(r, 'Use default', null),
            notifyItem(r, 'All messages', 'All'),
            notifyItem(r, 'Mentions and keywords only', 'Mentions'),
            notifyItem(r, 'Mute', 'Mute'),
          ],
        },
      },
      {
        id: 'tags',
        label: 'Tags',
        icon: Tag,
        submenu: {
          items: () =>
            tags.defs.map((def): MenuItem => {
              const on = liveTags(r.id).includes(def.id);
              return {
                id: `tag-${def.id}`,
                label: def.name,
                selected: on,
                keepOpen: true,
                onselect: () => void tags.setForRoom(r.id, def.id, !on),
              };
            }),
          search: {
            placeholder: 'Search or create a tag',
            create: { label: (q) => `Create tag "${q}"`, onCreate: (q) => void createTagFor(r.id, q) },
          },
          maxHeight: 320,
        },
      },
      { separator: true },
      {
        id: 'room-settings',
        label: 'Room settings',
        icon: Settings,
        submenu: {
          items: [
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
          ],
        },
      },
      {
        id: 'copy',
        label: 'Copy',
        icon: Copy,
        submenu: {
          items: [
            { id: 'copy-link', label: 'Copy room link', icon: Link, onselect: () => void copyText(`https://matrix.to/#/${r.id}`).catch(() => undefined) },
            { id: 'copy-id', label: 'Copy room ID', icon: Copy, onselect: () => void copyText(r.id).catch(() => undefined) },
          ],
        },
      },
      { separator: true },
      { id: 'leave', label: 'Leave room', icon: LogOut, danger: true, onselect: () => (pendingLeave = { id: r.id, title: roomTitle(r) }) },
    ];
  }

  function sortItems(): MenuItem[] {
    return ROOM_SORTS.map((value): MenuItem => ({
      id: `sort-${value}`,
      label: SORT_LABELS[value],
      selected: prefs.roomSort === value,
      onselect: () => prefs.set('roomSort', value),
    }));
  }

  function openSortMenu(event: MouseEvent) {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.open(
      sortItems(),
      { x: box.left, y: box.bottom },
      { anchor: { x: box.left, y: box.top, width: box.width, height: box.height }, ariaLabel: 'Sort rooms' },
    );
  }

  /** Marks every joined room in `list` with unread messages as read, through the same path as the room menu. */
  function markAllRead(list: readonly RoomItem[]) {
    for (const r of list) if (r.membership !== 'Invited' && roomsStore.unreadOf(r) > 0) void markRead(r.id);
  }

  function hasUnread(list: readonly RoomItem[]): boolean {
    return list.some((r) => r.membership !== 'Invited' && roomsStore.unreadOf(r) > 0);
  }

  /** Pane header and blank list area: room creation, discovery and read state. */
  function paneItems(): MenuItem[] {
    const browse: MenuItem[] = viewport.isPhone
      ? []
      : [{ id: 'browse', label: 'Browse public rooms...', icon: Compass, onselect: () => onbrowse?.() }];
    return [
      { id: 'new-room', label: 'New room...', icon: Plus, onselect: () => onnew?.() },
      ...browse,
      { separator: true },
      {
        id: 'mark-all',
        label: 'Mark all as read',
        icon: CheckCheck,
        disabled: !hasUnread(roomsStore.rooms),
        onselect: () => markAllRead(roomsStore.rooms),
      },
    ];
  }

  /** Filter chip menu: read state for the rows in view, and a shortcut to show only this filter. */
  function filterItems(filter: (typeof ROOM_FILTERS)[number]): MenuItem[] {
    return [
      {
        id: 'mark-view',
        label: 'Mark all in view as read',
        icon: CheckCheck,
        disabled: !hasUnread(roomsStore.visible),
        onselect: () => markAllRead(roomsStore.visible),
      },
      { separator: true },
      {
        id: `only-${filter.value}`,
        label: `Show only ${filter.label}`,
        disabled: roomsStore.filter === filter.value,
        onselect: () => (roomsStore.filter = filter.value),
      },
    ];
  }

  function colourLabel(colour: TagColour): string {
    return colour.charAt(0).toUpperCase() + colour.slice(1);
  }

  /** Tag chip menu. A tag that only rooms carry (not in the catalogue) can just be added first. */
  function tagItems(id: string): MenuItem[] {
    const def = tags.defs.find((d) => d.id === id);
    if (!def) {
      return [{ id: 'add-tag', label: 'Add to my tags', icon: Plus, onselect: () => void tags.create(id, 'grey') }];
    }
    const index = tags.defs.indexOf(def);
    const count = roomsStore.rooms.filter((r) => r.tags.includes(id)).length;
    return [
      {
        id: 'only',
        label: 'Show only this tag',
        icon: Tag,
        selected: roomsStore.tagFilter === id,
        onselect: () => (roomsStore.tagFilter = roomsStore.tagFilter === id ? null : id),
      },
      { id: 'rename', label: 'Rename tag...', icon: Pencil, onselect: () => (renameTagId = id) },
      {
        id: 'colour',
        label: 'Colour',
        icon: Palette,
        submenu: {
          items: () =>
            TAG_COLOURS.map(
              (c): MenuItem => ({
                id: `colour-${c}`,
                label: colourLabel(c),
                selected: tags.defs.find((d) => d.id === id)?.colour === c,
                onselect: () => void tags.update(id, { colour: c }),
              }),
            ),
        },
      },
      { id: 'up', label: 'Move up', icon: ChevronUp, disabled: index === 0, onselect: () => void tags.move(id, -1) },
      {
        id: 'down',
        label: 'Move down',
        icon: ChevronDown,
        disabled: index === tags.defs.length - 1,
        onselect: () => void tags.move(id, 1),
      },
      { separator: true },
      { id: 'manage', label: 'Manage tags...', icon: Settings, onselect: () => onsettings?.('tags') },
      {
        id: 'delete',
        label: 'Delete tag',
        icon: Trash2,
        danger: true,
        onselect: () => (pendingTagDelete = { id, name: def.name, count }),
      },
    ];
  }

  async function confirmDeleteTag() {
    if (pendingTagDelete) await tags.remove(pendingTagDelete.id);
  }

  function inviteItems(r: RoomItem): MenuItem[] {
    const busy = inviteBusy === r.id;
    return [
      { id: 'accept', label: 'Accept invitation', icon: Check, disabled: busy, onselect: () => void answerInvite(r.id, true) },
      { id: 'decline', label: 'Decline invitation', icon: X, disabled: busy, onselect: () => void answerInvite(r.id, false) },
    ];
  }

  const scopeSpace = $derived(ui.scope === 'home' ? null : (roomsStore.rooms.find((r) => r.id === ui.scope) ?? null));
  const scopeTitle = $derived(scopeSpace ? roomTitle(scopeSpace) : 'Messages');

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

  const renameTag = $derived(tags.defs.find((d) => d.id === renameTagId) ?? null);
  const tagDeleteMessage = $derived(
    pendingTagDelete
      ? `Remove tag ${pendingTagDelete.name} from ${pendingTagDelete.count} ${pendingTagDelete.count === 1 ? 'room' : 'rooms'}?`
      : '',
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
      <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size={36} shape={r.is_direct ? 'circle' : 'rounded'} />
    </span>
    <span class="text">
      <span class="title">{roomTitle(r)}</span>
      {#if prefs.roomListPreviews && r.latest_event}
        <span class="preview">{r.latest_event.body}</span>
      {/if}
    </span>
    <span class="end">
      {#if r.latest_event}
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
      <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size={36} shape={r.is_direct ? 'circle' : 'rounded'} />
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
  <header class="head" use:contextmenu={paneItems}>
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

  <div class="filters">
    <div class="chips" role="group" aria-label="Show rooms">
      {#each ROOM_FILTERS as f (f.value)}
        <button
          type="button"
          class="chip"
          use:contextmenu={() => filterItems(f)}
          class:active={roomsStore.filter === f.value}
          aria-pressed={roomsStore.filter === f.value}
          onclick={() => (roomsStore.filter = f.value)}
        >
          {f.label}
        </button>
      {/each}
    </div>
    <button type="button" class="icon sort" aria-label="Sort rooms" onclick={openSortMenu} use:contextmenu={sortItems}>
      <Icon icon={ArrowUpDown} size={16} />
    </button>
  </div>

  {#if tagChips.length > 0}
    <div class="tag-filter" role="toolbar" aria-label="Filter rooms by tag">
      {#each tagChips as chip (chip.def.id)}
        <div class="tag-chip" use:contextmenu={() => tagItems(chip.def.id)}>
          <TagChip
            label={chip.def.name}
            colour={chip.def.colour}
            count={chip.count}
            pressed={roomsStore.tagFilter === chip.def.id}
            onclick={() => (roomsStore.tagFilter = roomsStore.tagFilter === chip.def.id ? null : chip.def.id)}
          />
        </div>
      {/each}
    </div>
  {/if}

  {#if tags.error}
    <p class="error" role="alert">{tags.error}</p>
  {/if}

  <div class="groups" use:contextmenu={paneItems}>
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

{#if renameTag}
  <TagEditDialog open tag={renameTag} onclose={() => (renameTagId = null)} />
{/if}

<ConfirmDialog
  open={pendingTagDelete !== null}
  title="Delete tag"
  message={tagDeleteMessage}
  confirmLabel="Delete tag"
  danger
  onconfirm={confirmDeleteTag}
  onclose={() => (pendingTagDelete = null)}
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
  .search kbd {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .filters {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    margin: 0 var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
  }
  .chips {
    display: flex;
    flex: 1;
    min-width: 0;
    gap: var(--alloy-spacing-scale-1);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .chips::-webkit-scrollbar {
    display: none;
  }
  .filters .sort {
    flex: none;
  }
  .chip {
    flex: none;
    height: 26px;
    padding: 0 var(--alloy-spacing-scale-2);
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .chip:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .chip.active {
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
  }
  .tag-chip {
    display: contents;
  }
  .tag-filter {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-1);
    margin: 0 var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
  }
  .error {
    margin: 0 var(--alloy-layout-row-inset) var(--alloy-spacing-scale-2);
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
    font-weight: var(--alloy-typography-weight-medium);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-sm);
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
    color: var(--alloy-color-text-secondary);
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
    background: var(--alloy-color-accent-fill);
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
  /* The trailing column (time above the marks row) sits at the row's right edge on desktop and phone alike. */
  .end {
    display: flex;
    flex: none;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-1);
  }
  .marks {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-height: 20px;
  }
  .time {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .row.unread .time {
    color: var(--alloy-color-accent-primary);
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
    .filters,
    .tag-filter,
    .error {
      margin-left: var(--alloy-layout-gutter);
      margin-right: var(--alloy-layout-gutter);
    }
    .chip {
      min-height: 44px;
      min-width: 44px;
      height: auto;
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
      min-height: 44px;
      box-sizing: border-box;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    button.row:active {
      background: var(--alloy-color-surface-hover);
    }
    .marks :global(.alloy-badge) {
      min-width: 20px !important;
      height: 20px !important;
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
