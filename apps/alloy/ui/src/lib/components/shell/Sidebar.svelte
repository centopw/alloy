<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { Avatar, Badge } from '../common';
  import ConfirmDialog from '../common/ConfirmDialog.svelte';
  import TagChip from '../tags/TagChip.svelte';
  import TagEditDialog from '../tags/TagEditDialog.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import NetworkIcon from '$lib/icons/NetworkIcon.svelte';
  import { Archive, ArchiveRestore, ArrowUpDown, Bell, BellOff, CheckCheck, ChevronDown, ChevronUp, Copy, Ellipsis, Info, Mail, Palette, Pencil, Pin, PinOff, RefreshCw, Search, Settings, StickyNote, Tag, Trash2 } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { pullToRefresh } from '$lib/platform/pullToRefresh';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { isRoomMuted, MUTE_FOREVER, networkName, ROOM_FILTERS, roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { displayTags, TAG_COLOUR_VARS, TAG_COLOURS } from '$lib/stores/tags';
  import { tags } from '$lib/stores/tags.svelte';
  import { connectivity } from '$lib/stores/connectivity.svelte';
  import { refreshStore } from '$lib/stores/refresh.svelte';
  import UpdateNotice from '../updates/UpdateNotice.svelte';
  import { groupRoomsByTag, ROOM_SORTS, sortForGroupToggle, type RoomSort } from '$lib/stores/roomSort';
  import { rovingTabStop, rovingTargetIndex } from '$lib/platform/rovingFocus';
  import { roomRowSelector } from '$lib/platform/shellFocus';
  import type { TagColour } from '$lib/generated';
  import { ui } from '$lib/stores/ui.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { formatListTime } from '$lib/content/format';
  import { highlightRuns } from '$lib/content/searchHighlight';
  import { copyItems, notifyItems, openMenuFrom } from './roomMenus';
  import { movePinned, orderChanged } from './pinOrder';
  import RoomSwipe from './RoomSwipe.svelte';
  import { draftsStore } from '$lib/stores/drafts.svelte';
  import { muteRoom, unmuteRoom } from '$lib/stores/muteTimer.svelte';
  import { resolveAvatar } from '$lib/stores/avatars.svelte';
  import { globalSearch, isSearchable } from '$lib/stores/globalSearch.svelte';
  import { forumTotals, topicsOf } from './forumTopics';
  import { muteEndLabel } from '$lib/content/mute';

  import { t } from '$lib/i18n';
  interface Props {
    onselect?: (roomId: string) => void;
    onsettings?: (section?: 'tags') => void;
  }
  let { onselect, onsettings }: Props = $props();

  type RoomItem = (typeof roomsStore.rooms)[number];

  /** What a forum row overrides on the shared chat row: its counted unread, preview and target. */
  interface RoomRowOptions {
    /** Unread count to show; a forum shows the sum for itself and its topics. */
    unread?: number;
    /** Whether the row reads as the open chat; a forum counts as open while a topic of it is. */
    active?: boolean;
    /** Preview line to show instead of the room's own last activity. */
    preview?: string | null;
    /** Room the row opens; a forum opens its most recently active topic. */
    openId?: string;
  }

  /** Resolved chat avatars by room id; null while a lookup runs. */
  let avatars = $state<Record<string, string | null>>({});
  let renameTagId = $state<string | null>(null);
  let pendingTagDelete = $state<{ id: string; name: string; count: number } | null>(null);
  const SORT_LABELS: Record<RoomSort, string> = {
    activity: t('sidebar-sort-activity'),
    name: t('sidebar-sort-name'),
    unread: t('sidebar-sort-unread-first'),
    tag: t('sidebar-sort-by-tag'),
  };
  // The three filters people actually flip between; the rest stay in the More menu.
  const PRIMARY_FILTERS = ROOM_FILTERS.filter(
    (f) => f.value === 'all' || f.value === 'unread' || f.value === 'direct',
  );

  function openRoom(roomId: string) {
    roomsStore.select(roomId);
    onselect?.(roomId);
  }

  function showRoomInfo(roomId: string) {
    openRoom(roomId);
    ui.openRoomInfo('info');
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
    const unread = roomsStore.unreadOf(r);
    const muted = isRoomMuted(r);
    // Pinning needs the network's support (`pin_chats`); an already pinned chat can always be unpinned.
    const canPin = r.pinned || roomsStore.capabilitiesFor(r.account_id)?.pin_chats !== false;
    return [
      { id: 'mark-unread', label: unread > 0 || r.marked_unread ? t('common-mark-as-read') : t('sidebar-mark-as-unread'), icon: unread > 0 || r.marked_unread ? CheckCheck : Mail, onselect: () => (unread > 0 ? roomsStore.markRead(r.id) : void roomsStore.setMarkedUnread(r.id, !r.marked_unread)) },
      ...(canPin
        ? [{ id: 'pin', label: r.pinned ? t('sidebar-unpin-from-top') : t('sidebar-pin-to-top'), icon: r.pinned ? PinOff : Pin, onselect: () => void roomsStore.setPinned(r.id, !r.pinned) }]
        : []),
      { id: 'archive', label: r.archived ? t('room-swipe-unarchive') : t('room-swipe-archive'), icon: r.archived ? ArchiveRestore : Archive, onselect: () => void roomsStore.setArchived(r.id, !r.archived) },
      { id: 'mute', label: muted ? t('room-swipe-unmute') : t('room-swipe-mute'), icon: BellOff, onselect: () => void (muted ? unmuteRoom(r.id) : muteRoom(r.id, null)) },
      { separator: true },
      {
        id: 'notifications',
        label: t('sidebar-notifications'),
        icon: Bell,
        submenu: {
          items: () => notifyItems(r.id),
        },
      },
      {
        id: 'tags',
        label: t('sidebar-tags'),
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
            placeholder: t('sidebar-search-or-create-a-tag'),
            create: { label: (q) => t('sidebar-create-tag', { name: q }), onCreate: (q) => void createTagFor(r.id, q) },
          },
          maxHeight: 320,
        },
      },
      { separator: true },
      {
        id: 'room-settings',
        label: t('sidebar-room-settings'),
        icon: Settings,
        submenu: {
          items: [
            { id: 'details', label: t('common-room-details'), icon: Info, onselect: () => showRoomInfo(r.id) },
            { id: 'note', label: t('sidebar-edit-note'), icon: StickyNote, onselect: () => showRoomInfo(r.id) },
          ],
        },
      },
      {
        id: 'copy',
        label: t('common-copy'),
        icon: Copy,
        submenu: {
          items: copyItems(r),
        },
      },
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

  /** Marks every room in `list` with unread messages as read, through the same path as the room menu. */
  function markAllRead(list: readonly RoomItem[]) {
    for (const r of list) if (roomsStore.unreadOf(r) > 0) roomsStore.markRead(r.id);
  }

  function hasUnread(list: readonly RoomItem[]): boolean {
    return list.some((r) => roomsStore.unreadOf(r) > 0);
  }

  /** Pane header and blank list area: read state of everything the pane shows. */
  function paneItems(): MenuItem[] {
    return [
      {
        id: 'mark-all',
        label: t('common-mark-all-as-read'),
        icon: CheckCheck,
        disabled: !hasUnread(roomsStore.visible),
        onselect: () => markAllRead(roomsStore.visible),
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
        label: t('sidebar-sort-rooms'),
        icon: ArrowUpDown,
        submenu: {
          items: () => sortItems(),
        },
      },
      { separator: true },
      {
        id: 'mark-view',
        label: t('sidebar-mark-all-in-view-as-read'),
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
      return [{ id: 'add-tag', label: t('sidebar-add-to-my-tags'), icon: Pin, onselect: () => void tags.create(id, 'grey') }];
    }
    const index = tags.defs.indexOf(def);
    const count = roomsStore.rooms.filter((r) => r.tags.includes(id)).length;
    return [
      {
        id: 'only',
        label: t('sidebar-show-only-this-tag'),
        icon: Tag,
        selected: roomsStore.tagFilter === id,
        onselect: () => (roomsStore.tagFilter = roomsStore.tagFilter === id ? null : id),
      },
      { id: 'rename', label: t('sidebar-rename-tag'), icon: Pencil, onselect: () => (renameTagId = id) },
      {
        id: 'colour',
        label: t('tag-edit-dialog-colour'),
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
      { id: 'up', label: t('sidebar-move-up'), icon: ChevronUp, disabled: index === 0, onselect: () => void tags.move(id, -1) },
      {
        id: 'down',
        label: t('sidebar-move-down'),
        icon: ChevronDown,
        disabled: index === tags.defs.length - 1,
        onselect: () => void tags.move(id, 1),
      },
      { separator: true },
      { id: 'manage', label: t('sidebar-manage-tags'), icon: Settings, onselect: () => onsettings?.('tags') },
      {
        id: 'delete',
        label: t('sidebar-delete-tag'),
        icon: Trash2,
        danger: true,
        onselect: () => (pendingTagDelete = { id, name: def.name, count }),
      },
    ];
  }

  async function confirmDeleteTag() {
    if (pendingTagDelete) await tags.remove(pendingTagDelete.id);
  }

  const filterLabel = $derived(ROOM_FILTERS.find((f) => f.value === roomsStore.filter)?.label ?? t('room-filter-all'));
  const moreFilterActive = $derived(!PRIMARY_FILTERS.some((f) => f.value === roomsStore.filter));
  // The same count the shell badge shows: a forum counts once for its topics (`unreadChatCount`).
  const unreadRoomsCount = $derived(roomsStore.unreadChatCount);
  const multipleAccounts = $derived(roomsStore.accounts.length > 1);

  const joined = $derived(roomsStore.visible);
  // One row per forum: topics live beside the open conversation, not inline in the list. A topic
  // whose forum the filter dropped (search, chip) keeps its own row so it never disappears.
  const topLevel = $derived.by(() => {
    const listed = new Set(joined.map((r) => r.id));
    return joined.filter((r) => !r.parent_id || !listed.has(r.parent_id));
  });
  // The forum row stays highlighted while one of its topics is open.
  const activeForumId = $derived(roomsStore.active?.parent_id ?? null);
  // Pinned chats sit above everything else; the Archive view lists them in place.
  const pinned = $derived(roomsStore.filter === 'archive' ? [] : topLevel.filter((r) => r.pinned));
  const unpinned = $derived(roomsStore.filter === 'archive' ? topLevel : topLevel.filter((r) => !r.pinned));
  // One list for people and groups alike: pinned chats first, then every other chat in the store's order.
  const flat = $derived([...pinned, ...unpinned]);
  const searching = $derived(isSearchable(roomsStore.query));
  const nothing = $derived(roomsStore.visible.length === 0 && !searching);

  // Typing in the search box also searches message text across chats.
  $effect(() => globalSearch.schedule(roomsStore.query));
  const tagDefs = $derived(displayTags(tags.defs, roomsStore.rooms));
  const tagById = $derived(new Map(tagDefs.map((d) => [d.id, d] as const)));
  const tagChips = $derived(tagDefs.map((def) => ({ def, count: roomsStore.rooms.filter((r) => r.tags.includes(def.id)).length })));

  // Roving tabindex: the desktop room list is a single Tab stop and arrows move between rows. The
  // ids are the `button.row` ids in visual order, so the model matches the markup in every grouping
  // mode (flat, tags). Phones keep every row a normal button (touch-first UI).
  let rovingId = $state<string | null>(null);
  const rovingIds = $derived.by(() => {
    const visibleRows = prefs.roomSort === 'tag' ? groupRoomsByTag(flat, tagDefs).flatMap((g) => g.rooms) : flat;
    return visibleRows.map((r) => r.id);
  });
  // The row that owns the Tab stop when nothing was focused yet: the open chat's own row, or the
  // forum row when the open chat is one of its topics.
  const rovingActiveId = $derived(
    roomsStore.activeRoomId !== null && rovingIds.includes(roomsStore.activeRoomId)
      ? roomsStore.activeRoomId
      : activeForumId,
  );
  const rovingStopId = $derived(rovingTabStop(rovingIds, rovingId, rovingActiveId));

  /** Remembers the row the user last focused, so Tab returns there instead of the top of the list. */
  function onRoomFocus(roomId: string) {
    rovingId = roomId;
  }

  /**
   * Arrow-key movement for the roving list. Enter/Space still activate the focused row natively;
   * Shift+F10 is left to the row's `use:contextmenu` action, which opens the same menu as the
   * per-row "more" button (one Tab stop per list instead of two per room).
   */
  function onRoomKeydown(e: KeyboardEvent, roomId: string) {
    if (viewport.isPhone || e.defaultPrevented || e.ctrlKey || e.metaKey) return;
    if (e.altKey) {
      // Alt+Arrow rearranges a pinned chat inside its account's pinned block.
      if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
      const room = roomsStore.rooms.find((r) => r.id === roomId);
      if (!room?.pinned) return;
      e.preventDefault();
      const block = pinnedBlockOf(room.account_id);
      const index = block.indexOf(roomId);
      if (index === -1) return;
      const target = e.currentTarget instanceof HTMLElement ? e.currentTarget : null;
      void reorderPinnedTo(room.account_id, roomId, index + (e.key === 'ArrowUp' ? -1 : 1));
      // The keyed row moves in the DOM; keep the keyboard on it.
      void tick().then(() => target?.focus());
      return;
    }
    const next = rovingTargetIndex(rovingIds.length, rovingIds.indexOf(roomId), e.key);
    if (next === null) return;
    const list = e.currentTarget instanceof Element ? e.currentTarget.closest('.groups') : null;
    const row = list?.querySelector<HTMLElement>(roomRowSelector(rovingIds[next]));
    if (!row) return;
    e.preventDefault();
    row.focus();
  }

  // Pinned chats can be rearranged by hand, but only inside the pinned block of their own account:
  // each network keeps its own pin order, so a Telegram chat has no place among Messenger pins.
  // The pinned block is the ordered pinned rows of one account as the list shows them now.
  function pinnedBlockOf(accountId: string): string[] {
    return pinned.filter((r) => r.account_id === accountId).map((r) => r.id);
  }

  /** Moves `roomId` to `toIndex` within its account's pinned block and tells the store. */
  function reorderPinnedTo(accountId: string, roomId: string, toIndex: number): Promise<void> {
    const before = pinnedBlockOf(accountId);
    const after = movePinned(before, roomId, toIndex);
    if (!orderChanged(before, after)) return Promise.resolve();
    return roomsStore.reorderPinned(accountId, after);
  }

  /** The pinned row being dragged, and where it would land: the row under the pointer plus the half it is over. */
  let dragging = $state<{ roomId: string; accountId: string } | null>(null);
  let dropTarget = $state<{ roomId: string; after: boolean } | null>(null);

  /** Whether `r` can accept the current drag: a pinned chat of the same account that is not the dragged one. */
  function acceptsDrop(r: RoomItem): boolean {
    return dragging !== null && r.pinned && r.account_id === dragging.accountId && r.id !== dragging.roomId;
  }

  function onRowDragStart(e: DragEvent, r: RoomItem) {
    if (viewport.isPhone || !r.pinned || prefs.roomSort === 'tag') {
      e.preventDefault();
      return;
    }
    dragging = { roomId: r.id, accountId: r.account_id };
    dropTarget = null;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      // Some webviews drop the drag without a payload; the id is also what the drop handler reads.
      e.dataTransfer.setData('text/plain', r.id);
    }
  }

  function onRowDragOver(e: DragEvent, r: RoomItem) {
    if (!acceptsDrop(r)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const after = e.clientY >= rect.top + rect.height / 2;
    if (dropTarget?.roomId !== r.id || dropTarget.after !== after) dropTarget = { roomId: r.id, after };
  }

  function onRowDragLeave(e: DragEvent, r: RoomItem) {
    // Leaving for a child element is not leaving the row.
    const next = e.relatedTarget;
    if (next instanceof Node && (e.currentTarget as HTMLElement).contains(next)) return;
    if (dropTarget?.roomId === r.id) dropTarget = null;
  }

  function onRowDrop(e: DragEvent, r: RoomItem) {
    if (!dragging || !acceptsDrop(r)) return;
    e.preventDefault();
    const { roomId, accountId } = dragging;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const after = e.clientY >= rect.top + rect.height / 2;
    const block = pinnedBlockOf(accountId);
    const from = block.indexOf(roomId);
    let to = block.indexOf(r.id) + (after ? 1 : 0);
    // Removing the dragged row first shifts every index behind it by one.
    if (from !== -1 && from < to) to -= 1;
    endDrag();
    void reorderPinnedTo(accountId, roomId, to);
  }

  function endDrag() {
    dragging = null;
    dropTarget = null;
  }

  // A filter on a tag that no longer exists (deleted here or elsewhere) would hide every room.
  $effect(() => {
    const filter = roomsStore.tagFilter;
    if (filter !== null && !tagById.has(filter)) roomsStore.tagFilter = null;
  });

  // Chat pictures are stored media; resolve them for the rows that have one.
  $effect(() => {
    for (const room of roomsStore.visible) {
      if (!room.avatar || Object.hasOwn(avatars, room.id)) continue;
      avatars = { ...avatars, [room.id]: null };
      void resolveAvatar(room.avatar, 36).then((url) => (avatars = { ...avatars, [room.id]: url }));
    }
  });

  // Drafts survive a restart: the list shows them once the rooms are known.
  $effect(() => {
    if (roomsStore.loaded) untrack(() => void draftsStore.loadAll());
  });

  const renameTag = $derived(tags.defs.find((d) => d.id === renameTagId) ?? null);
  const tagDeleteMessage = $derived(
    pendingTagDelete
      ? `Remove tag ${pendingTagDelete.name} from ${pendingTagDelete.count} ${pendingTagDelete.count === 1 ? 'chat' : 'chats'}?`
      : '',
  );
</script>

{#snippet roomRow(r: RoomItem, opts: RoomRowOptions)}
  {@const unread = opts.unread ?? roomsStore.unreadOf(r)}
  {@const active = opts.active ?? r.id === roomsStore.activeRoomId}
  {@const muted = isRoomMuted(r)}
  {@const markedUnread = !!r.marked_unread && unread === 0}
  {@const draft = active ? null : roomsStore.draftOf(r)}
  {@const until =
    r.muted_until !== null && r.muted_until < MUTE_FOREVER
      ? muteEndLabel(r.muted_until, Date.now(), (ms) => formatListTime(ms))
      : null}
  {@const reorderable = r.pinned && !viewport.isPhone && prefs.roomSort !== 'tag'}
  {@const firstTag = r.tags.length > 0 ? tagById.get(r.tags[0]) : undefined}
  {@const tagLabel = firstTag
    ? t('sidebar-tags-label', { tags: r.tags.map((id) => tagById.get(id)?.name ?? id).join(', ') })
    : ''}
  {@const dropBefore = dropTarget?.roomId === r.id && !dropTarget.after}
  {@const dropAfter = dropTarget?.roomId === r.id && dropTarget.after}
  <div
    class="row-wrap"
    class:dragging={dragging?.roomId === r.id}
    class:drop-before={dropBefore}
    class:drop-after={dropAfter}
  >
    <button
      type="button"
      class="row"
      class:active
      class:unread={(unread > 0 || markedUnread) && !muted}
      class:muted
      data-room-id={r.id}
      aria-current={active ? 'true' : undefined}
      aria-keyshortcuts={reorderable ? 'Shift+F10 Alt+ArrowUp Alt+ArrowDown' : 'Shift+F10'}
      tabindex={viewport.isPhone ? undefined : r.id === rovingStopId ? 0 : -1}
      draggable={reorderable ? 'true' : undefined}
      use:contextmenu={() => roomItems(r)}
      onclick={() => openRoom(opts.openId ?? r.id)}
      onfocus={() => onRoomFocus(r.id)}
      onkeydown={(e) => onRoomKeydown(e, r.id)}
      ondragstart={reorderable ? (e) => onRowDragStart(e, r) : undefined}
      ondragend={reorderable ? endDrag : undefined}
      ondragover={(e) => onRowDragOver(e, r)}
      ondragleave={(e) => onRowDragLeave(e, r)}
      ondrop={(e) => onRowDrop(e, r)}
    >
      <span class="pinned avatar">
        <Avatar userId={r.id} displayName={roomTitle(r)} src={avatars[r.id] ?? null} size={36} shape={r.is_direct ? 'circle' : 'rounded'} />
        {#if multipleAccounts}
          <span class="net"><NetworkIcon network={r.network} label={networkName(r.network)} /></span>
        {/if}
      </span>
      <span class="text">
        <span class="title">{roomTitle(r)}</span>
        {#if draft}
          <span class="preview"><span class="draft">{t('sidebar-draft')}</span> {draft}</span>
        {:else if opts.preview !== undefined}
          {#if opts.preview}<span class="preview">{opts.preview}</span>{/if}
        {:else if prefs.roomListPreviews && r.latest_event}
          <span class="preview">{r.latest_event.body}</span>
        {/if}
      </span>
      <!-- Time over at most one mark: unread badge, else mute, else marked-unread dot, else the
           first tag's colour. Two marks never share the column. -->
      <span class="meta">
        {#if r.latest_event}
          <time class="time" datetime={new Date(r.latest_event.timestamp_millis).toISOString()}>{formatListTime(r.latest_event.timestamp_millis)}</time>
        {/if}
        {#if unread > 0 || muted || markedUnread || firstTag}
          <span class="mark">
            {#if unread > 0}
              <Badge count={unread} variant={muted ? 'muted' : 'default'} />
            {:else if muted}
              <Icon icon={BellOff} size={16} label={until ?? t('sidebar-notifications-muted')} />
            {:else if markedUnread}
              <span class="dot accent" role="img" aria-label={t('sidebar-marked-as-unread')}></span>
            {:else if firstTag}
              <span class="tag-dot" role="img" aria-label={tagLabel} style:background={TAG_COLOUR_VARS[firstTag.colour ?? 'grey']}></span>
            {/if}
          </span>
        {/if}
      </span>
    </button>
    {#if !viewport.isPhone}
      <button
        type="button"
        class="more"
        tabindex="-1"
        aria-label={t('sidebar-room-actions', { room: roomTitle(r) })}
        data-tooltip={t('common-more')}
        onclick={(e) => openMenuFrom(e.currentTarget, roomItems(r), t('common-more'))}
      >
        <Icon icon={Ellipsis} size={16} />
      </button>
    {/if}
  </div>
{/snippet}

<!-- A forum is a container of conversations. Like Telegram, opening it lands in its most recently
     active topic and the rail beside the messages switches from there; a forum with no topics
     opens itself. Archived topics are chats of their own, so they stay out of the badge and the
     click. -->
{#snippet forumRow(r: RoomItem)}
  {@const topics = topicsOf(roomsStore.rooms, r.id).filter((t) => !t.archived)}
  {@const totals = forumTotals(r, topics, (x) => roomsStore.unreadOf(x))}
  {@const lastActivity = prefs.roomListPreviews ? r.latest_event?.body ?? null : null}
  {@const count = topics.length > 0 ? t('forum-topic-count', { count: topics.length }) : null}
  {@render roomRow(r, {
    unread: totals.notifications,
    active: r.id === roomsStore.activeRoomId || r.id === activeForumId,
    preview: [lastActivity, count].filter(Boolean).join(' · ') || null,
    openId: topics[0]?.id ?? r.id,
  })}
{/snippet}

{#snippet listRow(r: RoomItem)}
  {#if r.is_forum}
    {@render forumRow(r)}
  {:else}
    <RoomSwipe
      id={r.id}
      enabled={viewport.isPhone}
      archived={r.archived}
      muted={isRoomMuted(r)}
      onarchive={() => void roomsStore.setArchived(r.id, !r.archived)}
      onmute={() => void (isRoomMuted(r) ? unmuteRoom(r.id) : muteRoom(r.id, null))}
    >
      {@render roomRow(r, {})}
    </RoomSwipe>
  {/if}
{/snippet}

{#snippet roomList(list: RoomItem[])}
  {#if prefs.roomSort === 'tag'}
    {#each groupRoomsByTag(list, tagDefs) as group (group.tag?.id ?? '')}
      <section class="tag-group" aria-label={group.tag?.name ?? t('sidebar-untagged')}>
        <h4>
          <span>{group.tag?.name ?? t('sidebar-untagged')}</span>
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

<section class="sidebar" aria-label={t('sidebar-rooms')}>
  <header class="head" data-tauri-drag-region use:contextmenu={paneItems}>
    <!-- On phones the room list is a screen of its own, so its title is that view's h1; the chat's
         h1 is `display: none` while the list shows. On desktop the conversation owns the h1. -->
    {#if viewport.isPhone}
      <h1 class="scope">{t('sidebar-chats')}</h1>
    {:else}
      <h2 class="scope" data-tauri-drag-region>{t('sidebar-chats')}</h2>
    {/if}
    {#if unreadRoomsCount > 0}
      <span class="scope-meta" data-tauri-drag-region>{unreadRoomsCount} {t('sidebar-unread')}</span>
    {/if}
    <div class="actions">
      <button
        type="button"
        class="icon"
        aria-label={t('sidebar-refresh')}
        data-tooltip={refreshStore.busy ? t('sidebar-refreshing') : t('sidebar-refresh-with-shortcut', { shortcut: shortcutLabel('R') })}
        disabled={refreshStore.busy}
        onclick={() => void refreshStore.run()}
      >
        <span class:spin={refreshStore.busy}><Icon icon={RefreshCw} size={20} /></span>
      </button>
      <button
        type="button"
        class="icon"
        aria-label={t('common-settings')}
        data-tooltip={t('common-settings')}
        onclick={() => onsettings?.()}
      >
        <Icon icon={Settings} size={20} />
      </button>
    </div>
  </header>

  {#if refreshStore.busy}
    <p class="refreshing" role="status">{t('sidebar-refreshing-hellip')}</p>
  {:else if refreshStore.error && !connectivity.offline}
    <p class="error" role="alert">{refreshStore.error}</p>
  {/if}

  <div class="search">
    <Icon icon={Search} size={16} />
    <input
      type="search"
      placeholder={t('sidebar-search-everything')}
      aria-label={t('sidebar-search-rooms-and-messages')}
      bind:value={roomsStore.query}
    />
    {#if !viewport.isPhone}<kbd>{shortcutLabel('K')}</kbd>{/if}
  </div>

  <div class="filters">
    <div class="filter-tabs" role="group" aria-label={t('sidebar-filter-rooms')}>
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
      aria-label={moreFilterActive ? t('sidebar-room-list-options-filter', { filter: filterLabel }) : t('sidebar-room-list-options')}
      data-tooltip={t('sidebar-filter-and-sort')}
      use:contextmenu={filterMenuItems}
      onclick={(e) => openMenuFrom(e.currentTarget as HTMLElement, filterMenuItems(), t('sidebar-room-list-options'))}
    >
      <Icon icon={ChevronDown} size={16} />
      {#if moreFilterActive}
        <span class="tab-dot" aria-hidden="true"></span>
      {/if}
    </button>
  </div>

  {#if tagChips.length > 0}
    <div class="tag-filter" role="toolbar" aria-label={t('sidebar-filter-rooms-by-tag')}>
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
        label={t('sidebar-group')}
        title={prefs.roomSort === 'tag' ? t('sidebar-show-one-list') : t('sidebar-group-rooms-by-tag')}
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
      {#if flat.length > 0}
        <section class="group" aria-label={t('sidebar-rooms-by-tag')}>
          {@render roomList([...pinned, ...unpinned])}
        </section>
      {/if}
    {:else if flat.length > 0}
      <section class="group" aria-label={t('sidebar-room-list')}>
        {@render roomList([...pinned, ...unpinned])}
      </section>
    {/if}

    {#if searching}
      <section class="group" aria-label={t('sidebar-messages')}>
        <h3><span>{t('sidebar-messages')}</span><span class="section-count">{globalSearch.hits.length}</span></h3>
        {#if globalSearch.error}
          <p class="error" role="alert">{globalSearch.error}</p>
        {:else if globalSearch.loading && globalSearch.hits.length === 0}
          <p class="note" role="status">{t('sidebar-searching')}</p>
        {:else if globalSearch.hits.length === 0}
          <p class="note">{t('sidebar-no-messages-found')}</p>
        {:else}
          <ul>
            {#each globalSearch.hits as hit (hit.room_id + hit.event_id)}
              <li>
                <button type="button" class="hit" onclick={() => void globalSearch.open(hit, openRoom)}>
                  <span class="hit-meta">
                    <Avatar userId={hit.sender} displayName={hit.sender_display_name} size="sm" />
                    <span class="hit-room">{hit.room_name ?? t('sidebar-room')}<time datetime={new Date(hit.timestamp_millis).toISOString()}>{formatListTime(hit.timestamp_millis)}</time></span>
                  </span>
                  <span class="hit-body">
                    <span class="hit-sender">{hit.sender_display_name ?? hit.sender}:</span>
                    {#each highlightRuns(hit.body, globalSearch.query) as run, i (i)}{#if run.match}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if nothing}
      <p class="empty">
        {#if !roomsStore.loaded}{t('sidebar-loading-conversations')}{:else if roomsStore.query}{t('sidebar-no-matches')}{:else}{t('sidebar-nothing-here-yet')}{/if}
      </p>
    {/if}
  </div>

  <UpdateNotice />
</section>

{#if renameTag}
  <TagEditDialog open tag={renameTag} onclose={() => (renameTagId = null)} />
{/if}

<ConfirmDialog
  open={pendingTagDelete !== null}
  title={t('sidebar-delete-tag')}
  message={tagDeleteMessage}
  confirmLabel={t('sidebar-delete-tag')}
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
    container-type: inline-size;
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
  /* Desktop: the head starts after the macOS traffic lights (see layout.css); a no-op elsewhere. */
  @media (min-width: 900px) {
    .head {
      padding-left: max(var(--alloy-spacing-scale-2), var(--alloy-titlebar-lights-inset));
    }
  }
  /* Under the traffic lights the head keeps 84px on the left; below 255px the "Chats" word yields to the
     unread chip and refresh button. It stays in the accessibility tree; only its paint goes. */
  @container (max-width: 255px) {
    :root[data-titlebar='overlay']:not([data-fullscreen='true']) .scope {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
      white-space: nowrap;
    }
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
    /* `overflow-y: auto` alone leaves the horizontal axis scrollable, and one row wider than the
       list (a long muted title, a wide tag) then paints a scrollbar along the bottom of the list
       that eats a row's worth of height. The rows truncate their own text, so no row is ever
       meant to scroll sideways. */
    overflow-x: hidden;
    /* 4px here plus 4px of row padding is the 8px inset the row highlight has always had; making the
       row itself carry more of it is what buys the preview its width back. */
    padding: 0 var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
  }
  /* Section label: 11px caps, muted, 8px above and 4px below the heading, on the same left rail as
     the avatars (4px list inset + 4px label inset). */
  .group h3,
  .group h4 {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: 0.06em; /* token-audit: caps tracking for the 11px section label, not a size step */
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }
  .group > h3:first-child {
    padding-top: var(--alloy-spacing-scale-1);
  }
  .group h3 > span:first-child {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .section-count {
    padding: 1px var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-control);
    font-size: var(--alloy-typography-size-xs);
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
     and a bare `.fixed` class would turn the avatar container into a viewport-fixed box. */
  .pinned {
    flex: none;
    display: inline-flex;
  }
  .tag-dot {
    width: var(--alloy-spacing-scale-2);
    height: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
  }
  /* One row is avatar · text · meta. The first two columns are the same x on every row, so the
     avatars, the titles and the previews read as three rails down the list. The meta column is
     content-sized: a chat with no status mark gives that width back to the preview instead of
     reserving an empty gutter for a mark it does not have. */
  .row {
    position: relative;
    display: grid;
    grid-template-columns: 36px minmax(0, 1fr) auto;
    align-items: center;
    column-gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-width: 0;
    min-height: 56px;
    box-sizing: border-box;
    padding: 0 var(--alloy-spacing-scale-1);
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
  /* The open room keeps the solid accent rail, but its fill stays neutral: an accent wash under an
     accent unread badge is the one pair that reads as a rendering bug rather than a state. */
  button.row.active {
    background: var(--alloy-glass-control-hover);
    border-color: var(--alloy-glass-rim);
    color: var(--alloy-color-text-primary);
  }
  button.row.active::before {
    content: '';
    position: absolute;
    top: 50%;
    left: 0;
    width: 3px;
    height: var(--alloy-spacing-scale-5);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    translate: 0 -50%;
  }
  .row.unread {
    color: var(--alloy-color-text-primary);
  }
  /* The text column is two fixed lines — a 20px title over an 18px preview — and the meta column
     repeats that rhythm (20px time over an 18px mark), so both columns share one baseline grid
     and the preview never wraps to a third line. */
  .text {
    display: grid;
    grid-template-rows: 20px 18px;
    align-content: center;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    line-height: 20px;
    letter-spacing: -0.005em;
    color: var(--alloy-color-text-secondary);
  }
  /* Unread is hierarchy rather than a colour flip: a heavier title in full-contrast text and the
     accent timestamp, while a read row stays one step back on the same two lines. */
  .row.unread .title {
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  button.row.active .title {
    color: var(--alloy-color-text-primary);
  }
  /* The network a chat belongs to: a glyph on the avatar's corner, ringed in the list surface. */
  .avatar {
    position: relative;
  }
  .net {
    position: absolute;
    right: -3px;
    bottom: -3px;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-panel);
    color: var(--alloy-color-text-secondary);
  }
  .net:empty {
    display: none;
  }
  .preview {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-row);
    line-height: 18px;
    color: var(--alloy-color-text-muted);
  }
  .row.muted .preview,
  .row.muted .title {
    color: var(--alloy-color-text-muted);
  }
  .row.muted {
    color: var(--alloy-color-text-muted);
  }
  /* Time over one status mark, on the same two rows as the title and the preview, so the stamp
     sits on the title's baseline whether or not the row has a mark. The column is sized by its
     widest line, so a badge appearing under an existing timestamp does not move the preview. */
  .meta {
    display: grid;
    grid-template-rows: 20px 18px;
    align-content: center;
    justify-items: end;
    gap: var(--alloy-spacing-scale-1);
  }
  .time {
    font-size: var(--alloy-typography-size-xs);
    line-height: 20px;
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .row.unread .time {
    color: var(--alloy-color-accent-primary);
  }
  .mark {
    display: grid;
    place-items: center;
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
  .row-wrap {
    position: relative;
  }
  /* Rearranging pinned chats: the dragged row fades, and an accent rule marks the slot the drop would take. */
  .row-wrap.dragging {
    opacity: 0.45;
  }
  .row-wrap.drop-before::before,
  .row-wrap.drop-after::after {
    content: '';
    position: absolute;
    left: var(--alloy-spacing-scale-2);
    right: var(--alloy-spacing-scale-2);
    height: 2px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    pointer-events: none;
    z-index: 1;
  }
  .row-wrap.drop-before::before {
    top: -2px;
  }
  .row-wrap.drop-after::after {
    bottom: -2px;
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
    /* The hovered row hands its trailing column over to the more button. */
    .row-wrap:hover:has(> .more) .meta {
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
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-text-muted);
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
      padding: 0 var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
    }
    .group h3,
    .group h4 {
      display: flex;
      align-items: center;
      box-sizing: border-box;
      height: 40px;
      padding: 0 var(--alloy-spacing-scale-1);
    }
    .group > h3:first-child {
      padding-top: 0;
    }
    button.row {
      min-height: 56px;
      box-sizing: border-box;
    }
    button.row:active {
      background: var(--alloy-color-surface-hover);
    }
    .mark :global(.alloy-badge) {
      min-width: 20px !important;
      height: 20px !important;
    }
  }
</style>
