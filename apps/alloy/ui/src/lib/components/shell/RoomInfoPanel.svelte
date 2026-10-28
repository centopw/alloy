<script module lang="ts">
  import { client } from '$lib/ipc';
  import { localFileUrl } from '$lib/platform/files';
  import { expect } from '$lib/stores/errors';

  /** Thumbnail URLs keyed by mxc source, shared by every panel instance. */
  const thumbCache = new Map<string, Promise<string | null>>();

  export function thumbFor(source: string | null | undefined, mimeType: string | null): Promise<string | null> {
    if (!source) return Promise.resolve(null);
    const cached = thumbCache.get(source);
    if (cached) return cached;
    const promise = client
      .getMedia({ kind: 'Remote', data: { uri: source } }, 'thumbnail', {
        width: 480,
        height: 360,
        mime_type: mimeType,
      })
      .then((res) => localFileUrl(expect(res, 'MediaRetrieved').payload.local_path))
      .catch(() => null);
    thumbCache.set(source, promise);
    return promise;
  }
</script>

<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { Icon, X, ChevronLeft, Play, File, ExternalLink, Image, Download, Copy, Link, MessageSquare, User, Search, Lock, Bell, BellOff, Ban, ShieldAlert, UserMinus, Pencil } from '$lib/icons';
  import { formatBytes, formatDay } from '$lib/content/format';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { copyText } from '$lib/platform/clipboard';
  import { openExternal } from '$lib/platform/links';
  import { pushBack } from '$lib/platform/backstack';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { roomMedia } from '$lib/stores/roomMedia.svelte';
  import { roomNotify } from '$lib/stores/roomNotify.svelte';
  import { membersStore, memberLabel } from '$lib/stores/members.svelte';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import { session } from '$lib/stores/session.svelte';
  import { moderationOptions } from '$lib/content/moderation';
  import type { RoomInfoTab } from '$lib/stores/ui.svelte';
  import type { RoomMediaItem, RoomMediaKind } from '$lib/generated/protocol';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { roomsStore } from '$lib/stores/rooms.svelte';
  import { userProfile } from '$lib/stores/userProfile.svelte';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import type { RoomMemberDto } from '$lib/generated';
  import { Avatar, Button, ConfirmDialog, SegmentedControl, TextArea, TextField, UserName } from '../common';
  import { copyItems, notifyItems, notifyLabel, openMenuFrom, roomPictureItems } from './roomMenus';
  import { fetchMediaFile } from '$lib/content/mediaFetch';
  import MediaViewer from '../timeline/MediaViewer.svelte';

  type MediaView = 'media' | 'files' | 'links';
  type View = 'overview' | MediaView | 'members';

  interface Props {
    roomId: string;
    onclose: () => void;
    /** Opens in-room search. The search button is hidden when omitted. */
    onsearch?: () => void;
    initialTab?: RoomInfoTab;
  }

  let { roomId, onclose, onsearch, initialTab = 'info' }: Props = $props();

  const MEDIA_KIND: Record<MediaView, RoomMediaKind> = {
    media: 'Media',
    files: 'Files',
    links: 'Links',
  };

  const MEDIA_TITLE: Record<MediaView, string> = {
    media: 'Shared media',
    files: 'Files & documents',
    links: 'Shared links',
  };

  /** Rows shown on the overview before "View all" opens the full list. */
  const PREVIEW = { media: 6, files: 3, links: 3, members: 5 } as const;

  /** Shared-media tabs in display order. */
  const SHARED_TABS: { value: MediaView; label: string }[] = [
    { value: 'media', label: 'Media' },
    { value: 'links', label: 'Links' },
    { value: 'files', label: 'Files' },
  ];

  const EMPTY_TEXT: Record<MediaView, string> = {
    media: 'No shared media yet',
    files: 'No shared files yet',
    links: 'No shared links yet',
  };

  function isMediaView(v: View): v is MediaView {
    return v === 'media' || v === 'files' || v === 'links';
  }

  function viewAll(count: number, done: boolean): string {
    return done ? `View all (${count})` : 'View all';
  }

  let view = $state<View>('overview');

  // Seed the view from the prop on mount and whenever the parent changes it.
  $effect(() => {
    view = initialTab === 'info' ? 'overview' : initialTab;
  });

  // A sub-view registers a system Back handler so Back returns to the overview first (phone only).
  $effect(() => {
    if (view === 'overview') return;
    return pushBack(() => (view = 'overview'));
  });

  const title = $derived(view === 'overview' ? 'Details' : view === 'members' ? 'Members' : MEDIA_TITLE[view]);

  // Details and note state
  const details = $derived(roomInfo.details(roomId));
  let nameDraft = $state('');
  let topicDraft = $state('');
  let noteDraft = $state('');
  let noteDirty = $state(false);
  let noteSaving = $state(false);
  let noteSaved = $state(false);
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

  // Name and topic are read as text; the pencil opens the field that edits them.
  let editingName = $state(false);
  let editingTopic = $state(false);
  let editingNote = $state(false);
  let nameInput = $state<HTMLInputElement | null>(null);
  let topicInput = $state<HTMLTextAreaElement | null>(null);
  let noteInput = $state<HTMLTextAreaElement | null>(null);
  let nameEditBtn = $state<HTMLButtonElement | null>(null);
  let topicEditBtn = $state<HTMLButtonElement | null>(null);
  let noteEditBtn = $state<HTMLButtonElement | null>(null);

  // Drafts follow the room while their field is closed, so an edit is never overwritten mid-typing.
  $effect(() => {
    if (!editingName) nameDraft = details?.name ?? '';
    if (!editingTopic) topicDraft = details?.topic ?? '';
  });

  $effect(() => {
    if (editingName) nameInput?.focus();
  });

  $effect(() => {
    if (editingTopic) topicInput?.focus();
  });

  $effect(() => {
    if (editingNote) noteInput?.focus();
  });

  /** Keyboard edits swap the field back for the pencil; focus follows so the tab order stays put. */
  function refocus(button: () => HTMLElement | null) {
    void tick().then(() => button()?.focus());
  }

  $effect(() => {
    const stored = roomInfo.note(roomId);
    if (!noteDirty) noteDraft = stored;
  });

  $effect(() => {
    const id = roomId;
    untrack(() => void roomInfo.load(id));
    return () => {
      void flushNote(id);
    };
  });

  async function flushNote(id: string) {
    if (noteTimer !== undefined) {
      clearTimeout(noteTimer);
      noteTimer = undefined;
    }
    if (!noteDirty) return;
    const text = noteDraft;
    noteDirty = false;
    noteSaving = true;
    noteSaved = false;
    try {
      await roomInfo.saveNote(id, text);
      noteSaved = !noteDirty;
    } finally {
      noteSaving = false;
    }
  }

  function onNoteInput(value: string) {
    noteDraft = value;
    noteDirty = true;
    noteSaved = false;
    if (noteTimer !== undefined) clearTimeout(noteTimer);
    const id = roomId;
    noteTimer = setTimeout(() => void flushNote(id), 800);
  }

  function editNote() {
    noteDraft = roomInfo.note(roomId);
    noteDirty = false;
    editingNote = true;
  }

  function cancelNoteEdit() {
    if (noteTimer !== undefined) {
      clearTimeout(noteTimer);
      noteTimer = undefined;
    }
    noteDirty = false;
    noteDraft = roomInfo.note(roomId);
    editingNote = false;
  }

  async function saveNoteEdit() {
    if (!editingNote) return;
    editingNote = false;
    await flushNote(roomId);
  }

  async function commitName() {
    const d = details;
    if (!d) return;
    const next = nameDraft.trim();
    if (!next) {
      nameDraft = d.name ?? '';
      return;
    }
    if (next === (d.name ?? '')) return;
    await roomInfo.rename(roomId, next);
  }

  async function commitTopic() {
    const d = details;
    if (!d) return;
    const next = topicDraft.trim();
    if (next === (d.topic ?? '').trim()) return;
    await roomInfo.setTopic(roomId, next);
  }

  function editName() {
    if (!details?.can_set_name) return;
    nameDraft = details.name ?? '';
    editingName = true;
  }

  function cancelNameEdit() {
    nameDraft = details?.name ?? '';
    editingName = false;
  }

  async function saveName() {
    if (!editingName) return;
    editingName = false;
    await commitName();
  }

  function editTopic() {
    if (!details?.can_set_topic) return;
    topicDraft = details.topic ?? '';
    editingTopic = true;
  }

  function cancelTopicEdit() {
    topicDraft = details?.topic ?? '';
    editingTopic = false;
  }

  async function saveTopic() {
    if (!editingTopic) return;
    editingTopic = false;
    await commitTopic();
  }

  // Room notifications load so the bell shows the current level.
  $effect(() => {
    const id = roomId;
    untrack(() => void roomNotify.load(id));
  });

  const room = $derived(roomsStore.rooms.find((r) => r.id === roomId));

  // Media state. The overview previews all three kinds; a sub-view loads only its own kind.
  const mediaState = $derived({
    media: roomMedia.view(roomId, 'Media'),
    files: roomMedia.view(roomId, 'Files'),
    links: roomMedia.view(roomId, 'Links'),
  });
  let sharedTab = $state<MediaView>('media');
  const sharedCurrent = $derived(mediaState[sharedTab]);
  const sharedHasItems = $derived(SHARED_TABS.some(({ value }) => mediaState[value].items.length > 0));
  const current = $derived(isMediaView(view) ? mediaState[view] : null);
  let sentinel = $state<HTMLElement | null>(null);
  let viewer = $state<{
    uri: string;
    mimeType: string;
    body: string;
    mediaKind: 'image' | 'video' | 'audio';
  } | null>(null);
  let fileBusy = $state<string | null>(null);
  let fileError = $state<string | null>(null);

  $effect(() => {
    const id = roomId;
    const v = view;
    const kinds: MediaView[] = v === 'overview' ? ['media', 'files', 'links'] : isMediaView(v) ? [v] : [];
    untrack(() => {
      for (const k of kinds) void roomMedia.load(id, MEDIA_KIND[k]);
    });
  });

  // Infinite scroll: load the next page when the sentinel at the bottom becomes visible.
  $effect(() => {
    const el = sentinel;
    const v = view;
    if (!el || !isMediaView(v)) return;
    const kind = MEDIA_KIND[v];
    const io = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      const state = roomMedia.view(roomId, kind);
      if (!state.loading && !state.done) void roomMedia.loadMore(roomId, kind);
    });
    io.observe(el);
    return () => io.disconnect();
  });

  function openMedia(item: RoomMediaItem) {
    if (item.uri === null) return;
    viewer = {
      uri: item.uri,
      mimeType: item.mime_type ?? '',
      body: item.body,
      mediaKind: item.kind === 'video' ? 'video' : item.kind === 'audio' ? 'audio' : 'image',
    };
  }

  async function openFile(item: RoomMediaItem) {
    if (item.uri === null) return;
    fileBusy = item.event_id;
    fileError = null;
    try {
      const url = await fetchMediaFile(item.uri, item.mime_type ?? 'application/octet-stream');
      await openExternal(url);
    } catch (e) {
      fileError = errorMessage(e);
    } finally {
      fileBusy = null;
    }
  }

  function linkOf(item: RoomMediaItem): string {
    return item.urls?.[0] ?? item.body;
  }

  function hostOf(url: string): string {
    try {
      return new URL(url).hostname;
    } catch {
      return url;
    }
  }

  // Members state. The roster reloads on every open (forced, so never stale) and on retry;
  // cached rows stay visible until the reply lands.
  const members = $derived(membersStore.membersOf(roomId));
  let membersLoading = $state(true);
  let membersError = $state<string | null>(null);
  let membersActionError = $state<string | null>(null);
  // Bumping this re-runs the roster load (the retry button).
  let membersAttempt = $state(0);

  function byName(a: RoomMemberDto, b: RoomMemberDto): number {
    return memberLabel(a).localeCompare(memberLabel(b));
  }

  const joined = $derived(members.filter((m) => m.membership === 'Joined'));
  const groups = $derived(
    [
      { title: 'Admin', members: joined.filter((m) => m.power_level >= 100).sort(byName) },
      { title: 'Moderator', members: joined.filter((m) => m.power_level >= 50 && m.power_level < 100).sort(byName) },
      { title: 'Members', members: joined.filter((m) => m.power_level < 50).sort(byName) },
      { title: 'Invited', members: members.filter((m) => m.membership === 'Invited').sort(byName) },
      { title: 'Banned', members: members.filter((m) => m.membership === 'Banned').sort(byName) },
    ].filter((g) => g.members.length > 0),
  );
  const ordered = $derived(groups.flatMap((g) => g.members));

  $effect(() => {
    const id = roomId;
    void membersAttempt;
    let current = true;
    membersLoading = true;
    membersError = null;
    untrack(() => {
      void (async () => {
        try {
          await membersStore.loadMembers(id, true);
          if (current) membersLoading = false;
        } catch (e) {
          if (!current) return;
          membersError = errorMessage(e);
          membersLoading = false;
        }
      })();
    });
    return () => {
      current = false;
    };
  });

  /** Opens the profile card at the pointer, or at the row centre when the row was activated from the keyboard. */
  function openMemberProfile(e: MouseEvent, userId: string) {
    let at = { x: e.clientX, y: e.clientY };
    if (e.detail === 0) {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      at = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }
    membersActionError = null;
    profileCard.open(userId, roomId, at);
  }

  async function messageMember(userId: string) {
    membersActionError = null;
    try {
      roomsStore.select(await userProfile.startDirectMessage(userId));
    } catch (e) {
      membersActionError = errorMessage(e);
    }
  }

  let panel = $state<HTMLElement | null>(null);

  /** Centre of a member's row. The row stays mounted after its context menu closes, so it anchors the profile card. */
  function memberRowCentre(userId: string): { x: number; y: number } | null {
    const row = panel?.querySelector<HTMLElement>(`[data-user-id="${CSS.escape(userId)}"]`);
    if (!row) return null;
    const r = row.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }

  /** A moderation action waiting for confirmation. */
  type PendingAction = { kind: 'kick' | 'ban' | 'unban' | 'block'; member: RoomMemberDto };
  let pendingAction = $state<PendingAction | null>(null);

  const ACTION_COPY: Record<PendingAction['kind'], { title: string; confirm: string; message: (name: string) => string }> = {
    kick: { title: 'Remove from room', confirm: 'Remove', message: (n) => `${n} will be removed from this room. They can join again if the room allows it.` },
    ban: { title: 'Ban from room', confirm: 'Ban', message: (n) => `${n} will be removed and cannot rejoin until unbanned.` },
    unban: { title: 'Unban', confirm: 'Unban', message: (n) => `${n} will be able to join this room again.` },
    block: { title: 'Block user', confirm: 'Block', message: (n) => `You will no longer see messages from ${n} in any room. You can unblock them in Settings.` },
  };

  async function runAction() {
    const action = pendingAction;
    if (!action) return;
    const id = action.member.user_id;
    if (action.kind === 'kick') expect(await client.kickUser(roomId, id), 'UserKicked');
    else if (action.kind === 'ban') expect(await client.banUser(roomId, id), 'UserBanned');
    else if (action.kind === 'unban') expect(await client.unbanUser(roomId, id), 'UserUnbanned');
    else expect(await client.ignoreUser(id), 'UserIgnored');
    if (action.kind !== 'block') await membersStore.loadMembers(roomId, true);
  }

  async function setLevel(m: RoomMemberDto, level: number) {
    membersActionError = null;
    try {
      expect(await client.setPowerLevel(roomId, m.user_id, level), 'PowerLevelSet');
      await membersStore.loadMembers(roomId, true);
    } catch (e) {
      membersActionError = errorMessage(e);
    }
  }

  function memberMenu(m: RoomMemberDto): MenuItem[] {
    const mod = moderationOptions(membersStore.detailsOf(roomId), m, session.account?.user_id ?? null);
    const moderation: MenuItem[] = [];
    if (mod.levels.length > 0) {
      moderation.push({
        id: 'power',
        label: 'Change role',
        icon: ShieldAlert,
        submenu: {
          items: mod.levels.map((l): MenuItem => ({ id: `role-${l.level}`, label: `Make ${l.label.toLowerCase()}`, onselect: () => void setLevel(m, l.level) })),
        },
      });
    }
    if (mod.kick) moderation.push({ id: 'kick', label: 'Remove from room', icon: UserMinus, danger: true, onselect: () => (pendingAction = { kind: 'kick', member: m }) });
    if (mod.ban) moderation.push({ id: 'ban', label: 'Ban from room', icon: Ban, danger: true, onselect: () => (pendingAction = { kind: 'ban', member: m }) });
    if (mod.unban) moderation.push({ id: 'unban', label: 'Unban', icon: Ban, onselect: () => (pendingAction = { kind: 'unban', member: m }) });
    const isSelf = m.user_id === session.account?.user_id;
    return [
      {
        id: 'view-profile',
        label: 'View profile',
        icon: User,
        onselect: () => {
          const at = memberRowCentre(m.user_id);
          if (at) profileCard.open(m.user_id, roomId, at);
        },
      },
      { id: 'message', label: 'Message', icon: MessageSquare, onselect: () => void messageMember(m.user_id) },
      { separator: true },
      { id: 'copy-user-id', label: 'Copy user ID', icon: Copy, onselect: () => void copyText(m.user_id).catch(() => undefined) },
      ...(isSelf ? [] : [{ id: 'block', label: 'Block user', icon: Ban, danger: true, onselect: () => (pendingAction = { kind: 'block' as const, member: m }) }]),
      ...(moderation.length > 0 ? [{ separator: true } as MenuItem, ...moderation] : []),
    ];
  }
</script>

{#snippet mediaGrid(items: RoomMediaItem[])}
  <div class="media-grid">
    {#each items as item (item.event_id)}
      <button
        type="button"
        class="media-cell"
        aria-label="Open {item.body || item.kind}"
        title={item.body || item.kind}
        onclick={() => openMedia(item)}
      >
        {#await thumbFor(item.kind === 'video' ? item.thumbnail_uri : item.uri, item.kind === 'video' ? 'image/jpeg' : item.mime_type)}
          <span class="media-fallback"></span>
        {:then src}
          <span class="media-fallback"><Icon icon={Image} size={20} /></span>
          {#if src}
            <img
              class="media-img"
              src={src}
              alt=""
              loading="lazy"
              onerror={(e) => e.currentTarget.remove()}
            />
          {/if}
        {/await}
        {#if item.kind === 'video'}
          <span class="play-badge" aria-hidden="true"><Icon icon={Play} size={16} /></span>
        {/if}
      </button>
    {/each}
  </div>
{/snippet}

{#snippet fileRow(item: RoomMediaItem)}
  <li>
    <button
      type="button"
      class="row"
      aria-label="Download {item.body}"
      title="Download {item.body}"
      disabled={fileBusy === item.event_id}
      onclick={() => openFile(item)}
    >
      <span class="row-icon"><Icon icon={File} size={20} /></span>
      <span class="row-main">
        <span class="row-title">{item.body}</span>
        <span class="row-meta">
          {formatBytes(item.size_bytes ?? 0)} · {item.sender_display_name || item.sender} · {formatDay(item.timestamp_millis)}
        </span>
      </span>
      <Icon icon={Download} size={16} />
    </button>
  </li>
{/snippet}

{#snippet linkRow(item: RoomMediaItem)}
  {@const url = linkOf(item)}
  <li>
    <button type="button" class="row" aria-label="Open link {url}" title={url} onclick={() => void openExternal(url)}>
      <span class="row-icon"><Icon icon={Link} size={20} /></span>
      <span class="row-main">
        <span class="row-title link">{hostOf(url)}</span>
        <span class="row-meta">{url}</span>
        <span class="row-meta">{item.sender_display_name || item.sender} · {formatDay(item.timestamp_millis)}</span>
      </span>
      <Icon icon={ExternalLink} size={16} />
    </button>
  </li>
{/snippet}

{#snippet memberRow(m: RoomMemberDto)}
  <li>
    <button
      type="button"
      class="row"
      aria-label="Open profile for {memberLabel(m)}"
      data-user-id={m.user_id}
      onclick={(e) => openMemberProfile(e, m.user_id)}
      use:contextmenu={() => memberMenu(m)}
    >
      <Avatar userId={m.user_id} displayName={memberLabel(m)} src={m.avatar_url} size="md" shape="circle" />
      <span class="row-main">
        <UserName userId={m.user_id} displayName={memberLabel(m)} roomId={roomId} />
      </span>
    </button>
  </li>
{/snippet}

{#snippet sharedItems(kind: MediaView, items: RoomMediaItem[])}
  {#if kind === 'media'}
    {@render mediaGrid(items)}
  {:else}
    <ul class="list">
      {#each items as item (item.event_id)}
        {#if kind === 'files'}
          {@render fileRow(item)}
        {:else}
          {@render linkRow(item)}
        {/if}
      {/each}
    </ul>
  {/if}
{/snippet}

<section class="details" aria-label="Room info" bind:this={panel}>
  <header class="bar">
    {#if view !== 'overview'}
      <button type="button" class="icon-btn" aria-label="Back to details" title="Back to details" onclick={() => (view = 'overview')}>
        <Icon icon={ChevronLeft} size={20} />
      </button>
    {/if}
    <h2 class="title">{title}</h2>
    {#if view === 'overview' || !viewport.isPhone}
      <button type="button" class="icon-btn close" aria-label="Close room info" title="Close room info" onclick={onclose}>
        <Icon icon={viewport.isPhone ? ChevronLeft : X} size={20} />
      </button>
    {/if}
  </header>

  <div class="body">
    {#if fileError}
      <p class="notice" role="alert">{fileError}</p>
    {/if}
    {#if roomInfo.error(roomId)}
      <p class="notice" role="alert">{roomInfo.error(roomId)}</p>
    {/if}

    {#if view === 'overview'}
      <div class="identity">
        {#snippet avatarView()}
          <Avatar userId={roomId} displayName={details?.name ?? ''} src={details?.avatar_url ?? null} size={56} shape="rounded" />
        {/snippet}
        {#if details?.can_set_avatar}
          <button
            type="button"
            class="picture-btn"
            aria-label="Room picture options"
            title="Room picture options"
            onclick={(e) => openMenuFrom(e.currentTarget, roomPictureItems(roomId), 'Room picture')}
          >
            {@render avatarView()}
          </button>
        {:else}
          {@render avatarView()}
        {/if}
        <div class="identity-main">
          {#if editingName}
            <div class="editor">
              <TextField
                bind:value={nameDraft}
                bind:el={nameInput}
                maxlength={255}
                aria-label="Room name"
                onkeydown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    void saveName();
                    refocus(() => nameEditBtn);
                  } else if (e.key === 'Escape') {
                    e.preventDefault();
                    cancelNameEdit();
                    refocus(() => nameEditBtn);
                  }
                }}
                onblur={() => void saveName()}
              />
              <p class="hint">Enter to save · Esc to cancel</p>
            </div>
          {:else}
            <div class="editable">
              <h3 class="name">{details?.name || 'Unnamed room'}</h3>
              {#if details?.can_set_name}
                <button
                  type="button"
                  class="edit-btn"
                  aria-label="Edit room name"
                  title="Edit room name"
                  bind:this={nameEditBtn}
                  onclick={editName}
                >
                  <Icon icon={Pencil} size={16} />
                </button>
              {/if}
            </div>
          {/if}
          {#if details?.canonical_alias}<p class="id">{details.canonical_alias}</p>{/if}
          {#if details}
            <p class="meta">
              {#if details.encrypted}<Icon icon={Lock} size={16} />{/if}
              {details.encrypted ? 'End-to-end encrypted' : 'Not encrypted'} · {details.joined_members} members
            </p>
          {/if}
        </div>
      </div>

      <div class="actions">
        {#if onsearch}
          <Button variant="ghost" size="sm" icon={Search} onclick={onsearch}>
            {#snippet children()}Search{/snippet}
          </Button>
        {/if}
        {#if room}
          <Button variant="ghost" size="sm" icon={Copy} onclick={(e) => openMenuFrom(e.currentTarget, copyItems(room), 'Copy ID')}>
            {#snippet children()}Copy ID{/snippet}
          </Button>
        {/if}
        <button
          type="button"
          class="icon-btn"
          aria-label={notifyLabel(roomId)}
          title={notifyLabel(roomId)}
          onclick={(e) => openMenuFrom(e.currentTarget, notifyItems(roomId), 'Notifications')}
        >
          <Icon icon={roomNotify.isMuted(roomId) ? BellOff : Bell} size={20} />
        </button>
      </div>

      <section class="section" aria-labelledby="room-info-about">
        <h3 class="heading" id="room-info-about">About</h3>
        {#if editingTopic}
          <div class="editor">
            <TextArea
              rows={3}
              bind:value={topicDraft}
              bind:el={topicInput}
              aria-label="Room topic"
              onkeydown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  void saveTopic();
                  refocus(() => topicEditBtn);
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  cancelTopicEdit();
                  refocus(() => topicEditBtn);
                }
              }}
              onblur={() => void saveTopic()}
            />
            <p class="hint">Enter to save · Shift+Enter for a new line · Esc to cancel</p>
          </div>
        {:else}
          <div class="editable" class:is-empty={!details?.topic}>
            {#if details?.topic}
              <p class="value">{details.topic}</p>
            {:else}
              <p class="value empty-value">{details?.can_set_topic ? 'No topic yet' : 'No topic'}</p>
            {/if}
            {#if details?.can_set_topic}
              <button
                type="button"
                class="edit-btn"
                aria-label="Edit room topic"
                title="Edit room topic"
                bind:this={topicEditBtn}
                onclick={editTopic}
              >
                <Icon icon={Pencil} size={16} />
              </button>
            {/if}
          </div>
        {/if}
        {#if roomInfo.saving(roomId)}
          <p class="status" role="status">Saving…</p>
        {/if}
      </section>

      <section class="card" aria-label="Private note">
        <div class="card-head">
          <h3 class="heading">Private note</h3>
          <span class="pill">Only you</span>
        </div>
        {#if editingNote}
          <div class="editor">
            <TextArea
              rows={3}
              bind:value={noteDraft}
              bind:el={noteInput}
              aria-label="Private note"
              oninput={(e) => onNoteInput(e.currentTarget.value)}
              onkeydown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  void saveNoteEdit();
                  refocus(() => noteEditBtn);
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  cancelNoteEdit();
                  refocus(() => noteEditBtn);
                }
              }}
              onblur={() => void saveNoteEdit()}
            />
            <p class="hint">Enter to save · Shift+Enter for a new line · Esc to cancel</p>
          </div>
        {:else}
          <div class="editable" class:is-empty={!roomInfo.note(roomId)}>
            {#if roomInfo.note(roomId)}
              <p class="value">{roomInfo.note(roomId)}</p>
            {:else}
              <p class="value empty-value">A note only you can see</p>
            {/if}
            <button
              type="button"
              class="edit-btn"
              aria-label="Edit private note"
              title="Edit private note"
              bind:this={noteEditBtn}
              onclick={editNote}
            >
              <Icon icon={Pencil} size={16} />
            </button>
          </div>
        {/if}
        {#if noteSaving}
          <p class="status" role="status">Saving…</p>
        {:else if noteSaved}
          <p class="status" role="status">Saved</p>
        {/if}
      </section>

      {#if sharedHasItems}
        <section class="section" aria-labelledby="room-info-shared">
          <div class="section-head">
            <h3 class="heading" id="room-info-shared">Shared</h3>
            {#if sharedCurrent.items.length > 0}
              <Button variant="ghost" size="sm" onclick={() => (view = sharedTab)}>
                {#snippet children()}{viewAll(sharedCurrent.items.length, sharedCurrent.done)}{/snippet}
              </Button>
            {/if}
          </div>
          <SegmentedControl options={SHARED_TABS} value={sharedTab} label="Shared content" onchange={(v) => (sharedTab = v as MediaView)} />
          {#if sharedCurrent.items.length > 0}
            {@render sharedItems(sharedTab, sharedCurrent.items.slice(0, PREVIEW[sharedTab]))}
          {:else}
            <p class="empty">{EMPTY_TEXT[sharedTab]}</p>
          {/if}
        </section>
      {/if}

      {#if ordered.length > 0}
        <section class="section">
          <div class="section-head">
            <h3 class="heading">Members</h3>
            <Button variant="ghost" size="sm" onclick={() => (view = 'members')}>
              {#snippet children()}{viewAll(ordered.length, true)}{/snippet}
            </Button>
          </div>
          <ul class="list">
            {#each ordered.slice(0, PREVIEW.members) as m (m.user_id)}
              {@render memberRow(m)}
            {/each}
          </ul>
        </section>
      {/if}
    {:else if isMediaView(view) && current}
      {#if current.error}
        <p class="notice" role="alert">{current.error}</p>
      {/if}

      {@const kind = view as MediaView}
      {#if current.items.length > 0}
        {@render sharedItems(kind, current.items)}
      {:else if current.done && !current.loading}
        <p class="empty">{EMPTY_TEXT[kind]}</p>
      {/if}

      {#if current.loading}
        <p class="status" role="status">Loading…</p>
      {:else if !current.done && current.items.length > 0}
        <Button variant="ghost" size="sm" onclick={() => roomMedia.loadMore(roomId, MEDIA_KIND[view as MediaView])}>
          {#snippet children()}Load more{/snippet}
        </Button>
      {/if}
      <div class="sentinel" bind:this={sentinel} aria-hidden="true"></div>
    {:else}
      {#if membersError}
        <div class="notice" role="alert">
          <p>Could not load members: {membersError}</p>
          <Button variant="ghost" size="sm" onclick={() => (membersAttempt += 1)}>
            {#snippet children()}Try again{/snippet}
          </Button>
        </div>
      {/if}
      {#if membersActionError}
        <p class="notice" role="alert">{membersActionError}</p>
      {/if}
      {#if membersLoading && members.length === 0}
        <p class="status" role="status">Loading members…</p>
      {:else}
        {#each groups as group (group.title)}
          <section class="section">
            <h3 class="heading">{group.title} ({group.members.length})</h3>
            <ul class="list">
              {#each group.members as m (m.user_id)}
                {@render memberRow(m)}
              {/each}
            </ul>
          </section>
        {/each}
      {/if}
    {/if}
  </div>
</section>

{#if pendingAction}
  {@const copy = ACTION_COPY[pendingAction.kind]}
  <ConfirmDialog
    open
    title={copy.title}
    message={copy.message(memberLabel(pendingAction.member))}
    confirmLabel={copy.confirm}
    danger={pendingAction.kind !== 'unban'}
    onconfirm={runAction}
    onclose={() => (pendingAction = null)}
  />
{/if}

{#if viewer}
  <MediaViewer
    open={true}
    onclose={() => (viewer = null)}
    items={[{ eventId: null, uri: viewer.uri, mimeType: viewer.mimeType, body: viewer.body, mediaKind: viewer.mediaKind, sender: '', senderName: null, timestampMillis: 0 }]}
    index={0}
  />
{/if}

<style>
  .details {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-chat-rim);
    flex-shrink: 0;
  }

  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    text-overflow: ellipsis;
    white-space: nowrap;
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
    transition: background-color var(--alloy-motion-duration-fast) ease, color var(--alloy-motion-duration-fast) ease;
  }

  .icon-btn:hover {
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-primary);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
    padding: var(--alloy-layout-gutter);
  }

  .identity {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    min-width: 0;
  }

  .identity-main {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }

  .name {
    margin: 0;
    font-size: var(--alloy-typography-size-xl);
    font-weight: var(--alloy-typography-weight-semibold);
    line-height: var(--alloy-typography-line-height-tight);
    overflow-wrap: anywhere;
  }

  .id {
    margin: 0;
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
    overflow-wrap: anywhere;
  }

  .meta {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  /* A value is text until the pencil is used, so the panel reads as a summary, not a form. */
  .editable {
    display: flex;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-1);
    width: 100%;
    min-width: 0;
  }

  .editable .name,
  .editable .value {
    flex: 1;
    min-width: 0;
  }

  .value {
    margin: 0;
    font-size: var(--alloy-font-size-row);
    line-height: var(--alloy-typography-line-height-normal);
    color: var(--alloy-color-text-secondary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .empty-value {
    color: var(--alloy-color-text-muted);
  }

  .edit-btn {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    margin-top: -2px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: var(--alloy-color-text-muted);
    opacity: 0;
    cursor: pointer;
    transition:
      opacity var(--alloy-motion-duration-fast) ease,
      background-color var(--alloy-motion-duration-fast) ease,
      color var(--alloy-motion-duration-fast) ease;
  }

  .editable:hover .edit-btn,
  .editable:focus-within .edit-btn,
  .editable.is-empty .edit-btn {
    opacity: 1;
  }

  .edit-btn:hover {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }

  .edit-btn:focus-visible {
    opacity: 1;
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }

  /* Touch has no hover state, so the affordance is always visible there. The icon stays 24px but
     the box itself grows to the 44px minimum: the negative margins keep the text line unchanged. */
  @media (hover: none) {
    .edit-btn {
      width: 44px;
      height: 44px;
      margin: -10px; /* token-audit: grows the 24px pencil to a 44px touch target */
      opacity: 1;
    }
  }

  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    width: 100%;
    min-width: 0;
  }

  .hint {
    margin: 0;
    font-size: var(--alloy-font-size-meta);
    color: var(--alloy-color-text-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    padding-top: var(--alloy-layout-section-gap);
    border-top: 1px solid var(--alloy-chat-rim);
  }

  /* The shared-media switch hugs its labels instead of stretching to the panel width, which left a
     dead segment of empty track to the right of Files. */
  .section > :global(.alloy-segmented-control) {
    align-self: flex-start;
    max-width: 100%;
  }

  /* A sub-view opens on its first section; the header already draws the line above it. */
  .body > .section:first-child {
    padding-top: 0;
    border-top: 0;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
  }

  .heading {
    margin: 0;
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--alloy-letter-spacing-caps);
    color: var(--alloy-color-text-muted);
  }

  .picture-btn {
    display: inline-flex;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: none;
    cursor: pointer;
  }

  .picture-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-chat-rim);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-glass-control);
  }
  /* The 7% control plate over the pane drops muted 10.5-13px text to 4.03-4.34:1 on dark and
     phones. Secondary keeps the plate and clears 5:1 on every theme. */
  .card .heading,
  .card .empty-value,
  .card .status {
    color: var(--alloy-color-text-secondary);
  }

  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
  }

  .pill {
    padding: 0 var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-font-size-meta);
  }

  .status,
  .empty {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }

  .notice {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-font-size-preview);
  }

  .media-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--alloy-spacing-scale-1);
  }

  .media-cell {
    position: relative;
    aspect-ratio: 1;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    overflow: hidden;
    background: var(--alloy-color-surface-subtle);
    cursor: pointer;
    transition: opacity var(--alloy-motion-duration-fast) ease;
  }

  .media-cell:hover {
    opacity: 0.85;
  }

  .media-img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .media-fallback {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    color: var(--alloy-color-text-muted);
  }

  .play-badge {
    position: absolute;
    right: var(--alloy-spacing-scale-1);
    bottom: var(--alloy-spacing-scale-1);
    display: inline-flex;
    padding: var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-width: 0;
    min-height: 44px;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border: 0;
    border-radius: var(--alloy-radii-lg);
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--alloy-motion-duration-fast) ease;
  }

  .row:hover:not(:disabled) {
    background: var(--alloy-color-surface-hover);
  }

  .row:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  .row-icon {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--alloy-color-text-muted);
  }

  .row-main {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .row-title {
    overflow: hidden;
    font-size: var(--alloy-font-size-row);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-title.link {
    color: var(--alloy-color-text-link);
  }

  .row-meta {
    overflow: hidden;
    font-size: var(--alloy-font-size-meta);
    color: var(--alloy-color-text-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sentinel {
    height: 1px;
  }

  @media (max-width: 899px) {
    .details {
      position: fixed;
      inset: 0;
      z-index: 50;
      padding-top: var(--alloy-safe-area-top);
      padding-bottom: var(--alloy-safe-area-bottom);
    }

    .close {
      order: -1;
    }

    .icon-btn {
      width: var(--alloy-touch-target);
      height: var(--alloy-touch-target);
    }

    .body {
      padding: var(--alloy-layout-gutter);
    }

    .row {
      min-height: 56px;
    }
  }
</style>
