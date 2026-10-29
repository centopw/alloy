<script lang="ts">
  import { untrack } from 'svelte';
  import { Avatar, Badge } from '../common';
  import ConfirmDialog from '../common/ConfirmDialog.svelte';
  import TagChip from '../tags/TagChip.svelte';
  import TagEditDialog from '../tags/TagEditDialog.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Archive, ArchiveRestore, ArrowUpDown, Bell, BellOff, Check, CheckCheck, ChevronDown, ChevronUp, Compass, Copy, Ellipsis, Info, LogOut, Lock, Mail, Palette, Pencil, Pin, PinOff, Plus, RefreshCw, Search, Settings, StickyNote, Tag, Trash2, X } from '$lib/icons';
  import { client } from '$lib/ipc';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { pullToRefresh } from '$lib/platform/pullToRefresh';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { expect } from '$lib/stores/errors';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { ROOM_FILTERS, roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { roomNotify } from '$lib/stores/roomNotify.svelte';
  import { displayTags, TAG_COLOUR_VARS, TAG_COLOURS } from '$lib/stores/tags';
  import { tags } from '$lib/stores/tags.svelte';
  import { connectivity } from '$lib/stores/connectivity.svelte';
  import { refreshStore } from '$lib/stores/refresh.svelte';
  import UpdateNotice from '../updates/UpdateNotice.svelte';
  import { groupRoomsByTag, ROOM_SORTS, sortForGroupToggle, type RoomSort } from '$lib/stores/roomSort';
  import type { TagColour } from '$lib/generated';
  import { ui } from '$lib/stores/ui.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { formatListTime } from '$lib/content/format';
  import { highlightRuns } from '$lib/content/searchHighlight';
  import RenameRoomDialog from './RenameRoomDialog.svelte';
  import { copyItems, notifyItems, openMenuFrom } from './roomMenus';
  import RoomSwipe from './RoomSwipe.svelte';
  import { draftsStore } from '$lib/stores/drafts.svelte';
  import { muteRoom, setRoomLevel } from '$lib/stores/muteTimer.svelte';
  import { globalSearch, isSearchable } from '$lib/stores/globalSearch.svelte';
  import { muteEndLabel } from '$lib/content/mute';

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
  // The three filters people actually flip between; the rest stay in the More menu.
  const PRIMARY_FILTERS = ROOM_FILTERS.filter(
    (f) => f.value === 'all' || f.value === 'unread' || f.value === 'direct',
  );

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

  async function leaveRoom(roomId: string) {
    expect(await client.invoke({ action: 'LeaveRoom', params: { room_id: roomId } }), 'RoomLeft');
    if (roomsStore.activeRoomId === roomId) roomsStore.select(null);
  }

  async function confirmLeave() {
    if (pendingLeave) await leaveRoom(pendingLeave.id);
  }

  /** Tag ids of a room as they are now, so the Tags submenu reflects each toggle. */
  function liveTags(roomId: string): readonly string[] {
    return roomsStore.rooms.find((x) => x.id === roomId)?.tags ?? [];
  }

  async function createTagFor(roomId: string, name: string) {
    const created = await tags.create(name, 'blue');
    if (created) await roomsStore.setRoomTag(roomId, created.id, true);
  }

  function roomItems(r: RoomItem): MenuItem[] {
    void roomInfo.load(r.id);
    void roomNotify.load(r.id);
    const d = roomInfo.details(r.id);
    const unread = roomsStore.unreadOf(r);
    return [
      { id: 'mark-unread', label: unread > 0 || r.marked_unread ? 'Mark as read' : 'Mark as unread', icon: unread > 0 || r.marked_unread ? CheckCheck : Mail, onselect: () => (unread > 0 ? void markRead(r.id) : void roomsStore.setMarkedUnread(r.id, !r.marked_unread)) },
      { id: 'pin', label: r.favourite ? 'Unpin from top' : 'Pin to top', icon: r.favourite ? PinOff : Pin, onselect: () => void roomsStore.setFavourite(r.id, !r.favourite) },
      { id: 'archive', label: r.low_priority ? 'Unarchive' : 'Archive', icon: r.low_priority ? ArchiveRestore : Archive, onselect: () => void roomsStore.setArchived(r.id, !r.low_priority) },
      { separator: true },
      {
        id: 'notifications',
        label: 'Notifications',
        icon: Bell,
        submenu: {
          items: () => notifyItems(r.id),
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
                onselect: () => void roomsStore.setRoomTag(r.id, def.id, !on),
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
            { id: 'note', label: 'Edit note', icon: StickyNote, onselect: () => showRoomInfo(r.id) },
          ],
        },
      },
      {
        id: 'copy',
        label: 'Copy',
        icon: Copy,
        submenu: {
          items: copyItems(r),
        },
      },
      { separator: true },
      { id: 'leave', label: 'Leave room', icon: LogOut, danger: true, onselect: () => (pendingLeave = { id: r.id, title: roomTitle(r) }) },
    ];
  }

  /** The sort to return to when grouping is turned off; `null` until grouping is first entered. */
  let sortBeforeGroup = $state<RoomSort | null>(null);

  /** Grouping is a view, not an ordering: remember the sort it covers so turning it off restores it. */
  function setSort(value: RoomSort) {
    if (value === 'tag') {
      if (prefs.roomSort !== 'tag') sortBeforeGroup = prefs.roomSort;
    } else {
      sortBeforeGroup = value;
    }
    prefs.set('roomSort', value);
  }

  function sortItems(): MenuItem[] {
    return ROOM_SORTS.map((value): MenuItem => ({
      id: `sort-${value}`,
      label: SORT_LABELS[value],
      selected: prefs.roomSort === value,
      onselect: () => setSort(value),
    }));
  }

  /** Marks every joined room in `list` with unread messages as read, through the same path as the room menu. */
  function markAllRead(list: readonly RoomItem[]) {
    for (const r of list) if (r.membership !== 'Invited' && roomsStore.unreadOf(r) > 0) void markRead(r.id);
  }

  function hasUnread(list: readonly RoomItem[]): boolean {
    return list.some((r) => r.membership !== 'Invited' && roomsStore.unreadOf(r) > 0);
  }

  /** Header add button: start a room, or discover public ones on desktop. */
  function createItems(): MenuItem[] {
    // Public-room browsing lives here on every layout: the phone tab bar has no Discover tab, and
    // the pane menu is where a phone user looks for "find a room" now.
    return [
      { id: 'new-room', label: 'New room...', icon: Plus, onselect: () => onnew?.() },
      { id: 'browse', label: 'Browse public rooms...', icon: Compass, onselect: () => onbrowse?.() },
    ];
  }

  /** Pane header and blank list area: room creation, discovery and read state. */
  function paneItems(): MenuItem[] {
    return [
      ...createItems(),
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

  /** List controls: filter, sort, and the read state of what is in view. */
  function filterMenuItems(): MenuItem[] {
    return [
      ...ROOM_FILTERS.map(
        (f): MenuItem => ({
          id: `filter-${f.value}`,
          label: f.label,
          selected: roomsStore.filter === f.value,
          onselect: () => (roomsStore.filter = f.value),
        }),
      ),
      { separator: true },
      {
        id: 'sort',
        label: 'Sort rooms',
        icon: ArrowUpDown,
        submenu: {
          items: () => sortItems(),
        },
      },
      { separator: true },
      {
        id: 'mark-view',
        label: 'Mark all in view as read',
        icon: CheckCheck,
        disabled: !hasUnread(roomsStore.visible),
        onselect: () => markAllRead(roomsStore.visible),
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
  const scopeTitle = $derived(scopeSpace ? roomTitle(scopeSpace) : 'Chats');
  const filterLabel = $derived(ROOM_FILTERS.find((f) => f.value === roomsStore.filter)?.label ?? 'All');
  const moreFilterActive = $derived(!PRIMARY_FILTERS.some((f) => f.value === roomsStore.filter));
  const unreadRoomsCount = $derived(
    roomsStore.rooms.filter(
      (r) => !r.is_space && r.membership !== 'Invited' && roomsStore.unreadOf(r) > 0,
    ).length,
  );

  const invites = $derived(roomsStore.visible.filter((r) => r.membership === 'Invited'));
  const joined = $derived(roomsStore.visible.filter((r) => r.membership !== 'Invited'));
  // Pinned chats (Matrix favourites) sit above everything else; the Archive view lists them in place.
  const pinned = $derived(roomsStore.filter === 'archive' ? [] : joined.filter((r) => r.favourite));
  const unpinned = $derived(roomsStore.filter === 'archive' ? joined : joined.filter((r) => !r.favourite));
  // With sections, direct messages get their own People section when roomListPeople is on; otherwise they sit with Rooms.
  const rooms = $derived(unpinned.filter((r) => !prefs.roomListPeople || !r.is_direct));
  const direct = $derived(prefs.roomListPeople ? unpinned.filter((r) => r.is_direct) : []);
  // Without sections: invites first, then pinned chats, then every other joined room in the engine's activity order.
  const flat = $derived([...invites, ...pinned, ...unpinned]);
  const searching = $derived(isSearchable(roomsStore.query));
  const nothing = $derived(roomsStore.visible.length === 0 && !searching);

  // Typing in the search box also searches message text across rooms.
  $effect(() => globalSearch.schedule(roomsStore.query));
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

  // Drafts survive a restart: the list shows them once the rooms are known.
  $effect(() => {
    if (roomsStore.loaded) untrack(() => void draftsStore.loadAll());
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
  {@const markedUnread = !!r.marked_unread && unread === 0}
  {@const draft = active ? null : draftsStore.previewOf(r.id)}
  {@const until = muteEndLabel(prefs.mutedUntil[r.id], Date.now(), (ms) => formatListTime(ms))}
  <div class="row-wrap">
    <button
      type="button"
      class="row"
      class:active
      class:unread={(unread > 0 || markedUnread) && !muted}
      class:muted
      data-room-id={r.id}
      aria-current={active ? 'true' : undefined}
      use:contextmenu={() => roomItems(r)}
      onclick={() => openRoom(r.id)}
    >
      <span class="pinned">
        <Avatar userId={r.id} displayName={roomTitle(r)} src={r.avatar_url} size={36} shape={r.is_direct ? 'circle' : 'rounded'} />
      </span>
      <span class="text">
        <span class="title">{roomTitle(r)}</span>
        {#if draft}
          <span class="preview"><span class="draft">Draft:</span> {draft}</span>
        {:else if prefs.roomListPreviews && r.latest_event}
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
          {#if r.favourite}
            <span class="pinned"><Icon icon={Pin} size={16} label="Pinned" /></span>
          {/if}
          {#if r.encrypted}
            <span class="pinned"><Icon icon={Lock} size={16} label="Encrypted" /></span>
          {/if}
          {#if muted}
            <span class="pinned"><Icon icon={BellOff} size={16} label={until ?? 'Notifications muted'} /></span>
          {/if}
          {#if unread > 0}
            {#if muted}
              <span class="pinned"><span class="dot" aria-hidden="true"></span></span>
            {:else}
              <span class="pinned"><Badge count={unread} /></span>
            {/if}
          {:else if markedUnread}
            <span class="pinned"><span class="dot accent" role="img" aria-label="Marked as unread"></span></span>
          {/if}
        </span>
      </span>
    </button>
    {#if !viewport.isPhone}
      <button
        type="button"
        class="more"
        aria-label="Actions for {roomTitle(r)}"
        data-tooltip="More"
        onclick={(e) => openMenuFrom(e.currentTarget, roomItems(r), 'Room actions')}
      >
        <Icon icon={Ellipsis} size={16} />
      </button>
    {/if}
  </div>
{/snippet}

{#snippet inviteRow(r: RoomItem)}
  <div class="row invite" use:contextmenu={() => inviteItems(r)}>
    <span class="pinned">
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
    <RoomSwipe
      id={r.id}
      enabled={viewport.isPhone}
      archived={!!r.low_priority}
      muted={roomNotify.isMuted(r.id)}
      onarchive={() => void roomsStore.setArchived(r.id, !r.low_priority)}
      onmute={() => void (roomNotify.isMuted(r.id) ? setRoomLevel(r.id, null) : muteRoom(r.id, null))}
    >
      {@render roomRow(r)}
    </RoomSwipe>
  {/if}
{/snippet}

{#snippet roomList(list: RoomItem[])}
  {#if prefs.roomSort === 'tag'}
    {#each groupRoomsByTag(list, tagDefs) as group (group.tag?.id ?? '')}
      <section class="tag-group" aria-label={group.tag?.name ?? 'Untagged'}>
        <h4>
          <span>{group.tag?.name ?? 'Untagged'}</span>
          <span class="section-count">{group.rooms.length}</span>
        </h4>
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
    <h2 class="scope" title={scopeTitle}>{scopeTitle}</h2>
    {#if unreadRoomsCount > 0}
      <span class="scope-meta">{unreadRoomsCount} unread</span>
    {/if}
    <div class="actions">
      <button
        type="button"
        class="icon"
        aria-label="Refresh"
        data-tooltip={refreshStore.busy ? 'Refreshing' : `Refresh (${shortcutLabel('R')})`}
        disabled={refreshStore.busy}
        onclick={() => void refreshStore.run()}
      >
        <span class:spin={refreshStore.busy}><Icon icon={RefreshCw} size={20} /></span>
      </button>
      <button
        type="button"
        class="icon"
        aria-label="New room or browse"
        aria-haspopup="menu"
        data-tooltip="New room or browse"
        use:contextmenu={createItems}
        onclick={(e) => openMenuFrom(e.currentTarget as HTMLElement, createItems(), 'Create or browse')}
      >
        <Icon icon={Plus} size={20} />
      </button>
    </div>
  </header>

  {#if refreshStore.busy}
    <p class="refreshing" role="status">Refreshing&hellip;</p>
  {:else if refreshStore.error && !connectivity.offline}
    <p class="error" role="alert">{refreshStore.error}</p>
  {/if}

  <div class="search">
    <Icon icon={Search} size={16} />
    <input
      type="search"
      placeholder="Search everything"
      aria-label="Search rooms and messages"
      bind:value={roomsStore.query}
    />
    {#if !viewport.isPhone}<kbd>{shortcutLabel('K')}</kbd>{/if}
  </div>

  <div class="filters">
    <div class="filter-tabs" role="group" aria-label="Filter rooms">
      {#each PRIMARY_FILTERS as f (f.value)}
        <button
          type="button"
          class="filter-tab"
          class:on={roomsStore.filter === f.value}
          aria-pressed={roomsStore.filter === f.value}
          onclick={() => (roomsStore.filter = f.value)}
        >
          {f.label}
        </button>
      {/each}
    </div>
    <button
      type="button"
      class="filter-more"
      class:on={moreFilterActive}
      aria-pressed={moreFilterActive}
      aria-haspopup="menu"
      aria-label={moreFilterActive ? `Room list options, filter ${filterLabel}` : 'Room list options'}
      data-tooltip="Filter and sort"
      use:contextmenu={filterMenuItems}
      onclick={(e) => openMenuFrom(e.currentTarget as HTMLElement, filterMenuItems(), 'Room list options')}
    >
      <Icon icon={ChevronDown} size={16} />
      {#if moreFilterActive}
        <span class="tab-dot" aria-hidden="true"></span>
      {/if}
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
      <!-- Grouping lives beside the tags rather than only in the sort submenu: on a phone the list
           showed the chips but never the sections, because nothing in the header said they existed. -->
      <TagChip
        label="Group"
        title={prefs.roomSort === 'tag' ? 'Show one list' : 'Group rooms by tag'}
        pressed={prefs.roomSort === 'tag'}
        onclick={() => setSort(sortForGroupToggle(prefs.roomSort, sortBeforeGroup))}
      />
    </div>
  {/if}

  {#if tags.error && !connectivity.offline}
    <p class="error" role="alert">{tags.error}</p>
  {/if}

  <div class="groups" use:contextmenu={paneItems} use:pullToRefresh={{ onrefresh: () => void refreshStore.run(), enabled: viewport.isPhone }}>
    {#if prefs.roomSort === 'tag'}
      <!-- Tag groups are the sections. Splitting the same rooms into Pinned/Rooms/People first
           repeated every tag heading once per outer section (a room tagged Work showed up under
           "Work" twice), so grouping replaces those sections instead of nesting inside them. -->
      {#if invites.length > 0}
        <section class="group" aria-label="Invites">
          <h3><span>Invites</span><span class="section-count">{invites.length}</span></h3>
          <ul>
            {#each invites as r (r.id)}
              <li>{@render inviteRow(r)}</li>
            {/each}
          </ul>
        </section>
      {/if}
      {#if pinned.length + unpinned.length > 0}
        <section class="group" aria-label="Rooms by tag">
          {@render roomList([...pinned, ...unpinned])}
        </section>
      {/if}
    {:else if prefs.roomListSections}
      {#if invites.length > 0}
        <section class="group" aria-label="Invites">
          <h3><span>Invites</span><span class="section-count">{invites.length}</span></h3>
          <ul>
            {#each invites as r (r.id)}
              <li>{@render inviteRow(r)}</li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if pinned.length > 0}
        <section class="group" aria-label="Pinned">
          <h3><span>Pinned</span><span class="section-count">{pinned.length}</span></h3>
          {@render roomList(pinned)}
        </section>
      {/if}

      {#if rooms.length > 0}
        <section class="group" aria-label="Rooms">
          <h3><span>Rooms</span><span class="section-count">{rooms.length}</span></h3>
          {@render roomList(rooms)}
        </section>
      {/if}

      {#if direct.length > 0}
        <section class="group" aria-label="People">
          <h3><span>People</span><span class="section-count">{direct.length}</span></h3>
          {@render roomList(direct)}
        </section>
      {/if}
    {:else if flat.length > 0}
      <section class="group" aria-label="Room list">
        {@render roomList(flat)}
      </section>
    {/if}

    {#if searching}
      <section class="group" aria-label="Messages">
        <h3><span>Messages</span><span class="section-count">{globalSearch.hits.length}</span></h3>
        {#if globalSearch.error}
          <p class="error" role="alert">{globalSearch.error}</p>
        {:else if globalSearch.loading && globalSearch.hits.length === 0}
          <p class="note" role="status">Searching</p>
        {:else if globalSearch.hits.length === 0}
          <p class="note">No messages found.</p>
        {:else}
          <ul>
            {#each globalSearch.hits as hit (hit.room_id + hit.event_id)}
              <li>
                <button type="button" class="hit" onclick={() => void globalSearch.open(hit, openRoom)}>
                  <span class="hit-meta">
                    <Avatar userId={hit.sender} displayName={hit.sender_display_name} size="sm" />
                    <span class="hit-room">{hit.room_name ?? 'Room'}<time datetime={new Date(hit.timestamp_millis).toISOString()}>{formatListTime(hit.timestamp_millis)}</time></span>
                  </span>
                  <span class="hit-body">
                    <span class="hit-sender">{hit.sender_display_name ?? hit.sender}:</span>
                    {#each highlightRuns(hit.body, globalSearch.query) as run, i (i)}{#if run.match}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
          {#if globalSearch.nextBatch}
            <button type="button" class="more-hits" disabled={globalSearch.loading} onclick={() => void globalSearch.more()}>Show more</button>
          {/if}
        {/if}
        <p class="note">Encrypted rooms are not included.</p>
      </section>
    {/if}

    {#if nothing}
      <p class="empty">
        {#if !roomsStore.loaded}Loading conversations{:else if roomsStore.query}No matches{:else}Nothing here yet{/if}
      </p>
    {/if}
  </div>

  <UpdateNotice />
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
    background: transparent;
    color: var(--alloy-color-text-secondary);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-spacing-scale-2);
    border-bottom: 1px solid var(--alloy-chat-rim);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
    margin-left: auto;
  }
  .scope {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.015em;
    color: var(--alloy-color-text-primary);
  }
  /* A live read of the list: one glance answers "is anything waiting for me?". */
  .scope-meta {
    flex: none;
    padding: 2px var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-medium);
    white-space: nowrap;
  }
  /* A quiet field: the sidebar's quick-switcher, not a toolbar. */
  .search {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    height: 34px;
    margin: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2) 6px; /* token-audit: search box sits one step tighter than the 8px rhythm */
    padding: 0 var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-chat-rim);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-muted);
    transition:
      border-color var(--alloy-motion-duration-fast) ease,
      background var(--alloy-motion-duration-fast) ease;
  }
  .search:hover {
    border-color: var(--alloy-glass-rim-strong);
    background: var(--alloy-glass-control-hover);
  }
  .search:focus-within {
    border-color: color-mix(in srgb, var(--alloy-color-accent-primary) 55%, transparent);
    background: var(--alloy-glass-fill-strong);
    outline: 2px solid color-mix(in srgb, var(--alloy-color-accent-primary) 18%, transparent);
    outline-offset: 0;
  }
  .search input {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-font-size-preview);
  }
  .search input::placeholder {
    color: var(--alloy-color-text-muted);
  }
  .search input:focus {
    outline: none;
  }
  .search kbd {
    flex: none;
    padding: 3px 5px; /* token-audit: kbd chip inset, set by the 10px meta text */
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-sm);
    background: color-mix(in srgb, var(--alloy-color-text-primary) 4%, transparent);
    font-family: inherit;
    font-size: 10px;
    line-height: 1;
    color: var(--alloy-color-text-muted);
  }
  .filters {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    margin: 0 var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
  }
  /* A segmented control for the three views people flip between. Everything else — the remaining
     filters, sorting, bulk read state — sits behind the chevron so the list keeps the room. */
  .filter-tabs {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: 2px;
    min-width: 0;
    padding: 2px;
    border: 1px solid var(--alloy-chat-rim);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-glass-control);
  }
  .filter-tab {
    position: relative;
    display: inline-flex;
    flex: 1 1 0;
    align-items: center;
    justify-content: center;
    min-width: 0;
    height: 24px;
    padding: 0 var(--alloy-spacing-scale-2);
    overflow: hidden;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-font-size-preview);
    font-weight: var(--alloy-typography-weight-medium);
    white-space: nowrap;
    text-overflow: ellipsis;
    cursor: pointer;
    transition:
      background var(--alloy-motion-duration-fast) ease,
      color var(--alloy-motion-duration-fast) ease,
      box-shadow var(--alloy-motion-duration-fast) ease;
  }
  .filter-tab:hover {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  .filter-tab.on {
    background: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
  }
  /* The menu trigger is a quiet icon button beside the control, not a fourth segment: an empty
     segment made the switch look unfinished on wide screens. */
  .filter-more {
    position: relative;
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-muted);
    cursor: pointer;
    transition:
      background var(--alloy-motion-duration-fast) ease,
      color var(--alloy-motion-duration-fast) ease;
  }
  .filter-more:hover {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  .filter-more.on {
    background: var(--alloy-glass-control);
    color: var(--alloy-color-accent-primary);
  }
  /* A filter outside the segmented set is on; the dot is the only place that can show it. */
  .tab-dot {
    position: absolute;
    top: 3px;
    right: 3px;
    width: 4px;
    height: 4px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
  }
  .filter-tab:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
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
    padding: 0 var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
  }
  .group h3,
  .group h4 {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2) 2px;
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }
  .group > h3:first-child {
    padding-top: var(--alloy-spacing-scale-1);
  }
  .section-count {
    padding: 1px 6px; /* token-audit: kbd chip inset, set by the 10px meta text */
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-control);
    font-size: 10px;
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: 0;
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-secondary);
  }
  .group ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  /* Named `.pinned`, not `.fixed`: Tailwind's utilities layer emits a global `.fixed{position:fixed}`
     and a bare `.fixed` class would turn every avatar/marks container into a viewport-fixed box. */
  .pinned {
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
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-width: 0;
    min-height: 48px;
    box-sizing: border-box;
    padding: 6px var(--alloy-spacing-scale-2); /* token-audit: room row inset, tuned to the 34px row */
    border: 1px solid transparent;
    border-radius: var(--alloy-radii-lg);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    text-align: left;
    transition:
      background var(--alloy-motion-duration-fast) ease,
      border-color var(--alloy-motion-duration-fast) ease,
      box-shadow var(--alloy-motion-duration-fast) ease;
  }
  button.row {
    cursor: pointer;
  }
  button.row:hover {
    background: var(--alloy-glass-control);
  }
  /* The open room keeps a solid accent rail instead of a second glass card: on a glass pane,
     another translucent fill reads as a smudge, while the rail reads as a fixed position. */
  button.row.active {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 9%, transparent);
    color: var(--alloy-color-text-primary);
  }
  button.row.active::before {
    content: '';
    position: absolute;
    top: 50%;
    left: 3px;
    width: 3px;
    height: 20px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    translate: 0 -50%;
  }
  .row.unread {
    color: var(--alloy-color-text-primary);
  }
  .row.unread .title {
    color: var(--alloy-color-text-primary);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.005em;
  }
  .row:not(.unread):not(.active) .title {
    font-weight: var(--alloy-typography-weight-medium);
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
    font-size: var(--alloy-font-size-preview);
    line-height: 1.35;
    color: var(--alloy-color-text-secondary);
  }
  .row.muted .preview,
  .row.muted .title {
    color: var(--alloy-color-text-muted);
  }
  .row.muted {
    color: var(--alloy-color-text-muted);
  }
  .note {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .hit {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    min-height: 44px;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border: 0;
    border-radius: var(--alloy-glass-radius-control);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .hit:hover {
    background: var(--alloy-glass-control);
  }
  .hit-meta {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }
  .hit-meta .hit-room {
    flex: 1;
    min-width: 0;
  }
  .hit mark {
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
  }
  .hit-room {
    display: flex;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    font-weight: var(--alloy-typography-weight-semibold);
    font-size: var(--alloy-font-size-row);
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
  .more-hits {
    width: 100%;
    min-height: 36px;
    border: 0;
    background: transparent;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    cursor: pointer;
  }
  .row-wrap {
    position: relative;
  }
  .more {
    position: absolute;
    top: 50%;
    right: var(--alloy-spacing-scale-2);
    display: none;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-fill-strong);
    color: var(--alloy-color-text-secondary);
    translate: 0 -50%;
    cursor: pointer;
  }
  @media (hover: hover) {
    .row-wrap:hover .more,
    .more:focus-visible {
      display: grid;
    }
    .row-wrap:hover .end {
      visibility: hidden;
    }
  }
  .draft {
    color: var(--alloy-color-accent-primary);
  }
  .dot.accent {
    background: var(--alloy-color-accent-primary);
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
  .refreshing {
    margin: 0;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-3);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
  }
  .spin {
    display: grid;
    animation: sidebar-spin 900ms linear infinite;
  }
  @keyframes sidebar-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .icon:hover:not(:disabled) {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  .icon:disabled {
    cursor: default;
    opacity: 0.55;
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
    font-size: var(--alloy-font-size-meta);
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .row.unread .time {
    color: var(--alloy-color-accent-primary);
    font-weight: var(--alloy-typography-weight-semibold);
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
    .filter-tabs {
      gap: var(--alloy-spacing-scale-1);
      padding: 0;
      border: 0;
      background: transparent;
    }
    .filter-tab {
      height: 44px;
      padding: 0 var(--alloy-spacing-scale-3);
      border-radius: var(--alloy-radii-full);
      font-size: var(--alloy-font-size-row);
    }
    .filter-more {
      width: 44px;
      height: 44px;
      border-radius: var(--alloy-radii-full);
    }
    .groups {
      padding: 0 calc(var(--alloy-layout-gutter) - var(--alloy-layout-row-pad-x)) var(--alloy-spacing-scale-2);
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
    }
    .group h3,
    .group h4 {
      display: flex;
      align-items: center;
      box-sizing: border-box;
      height: 40px;
      padding: 0 var(--alloy-layout-row-pad-x);
    }
    .group > h3:first-child {
      padding-top: 0;
    }
    button.row {
      min-height: 56px;
      box-sizing: border-box;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    .title {
      font-size: var(--alloy-typography-size-base);
    }
    .preview {
      font-size: var(--alloy-font-size-row);
    }
    .time {
      font-size: var(--alloy-font-size-preview);
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
