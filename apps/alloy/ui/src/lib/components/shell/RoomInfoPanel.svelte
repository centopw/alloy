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
  import { untrack } from 'svelte';
  import { Icon, X, ChevronLeft, Play, File, Link, ExternalLink, Image, ImagePlus, Trash2, Download, Copy, MessageSquare, User } from '$lib/icons';
  import { formatBytes, formatDay } from '$lib/content/format';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { pickFile } from '$lib/platform/files';
  import { openExternal } from '$lib/platform/links';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { roomMedia } from '$lib/stores/roomMedia.svelte';
  import { membersStore, memberLabel } from '$lib/stores/members.svelte';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import type { RoomMediaItem, RoomMediaKind } from '$lib/generated/protocol';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { roomsStore } from '$lib/stores/rooms.svelte';
  import { userProfile } from '$lib/stores/userProfile.svelte';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import type { RoomMemberDto } from '$lib/generated';
  import { Avatar, Button, TextArea, TextField, UserName } from '../common';
  import MediaViewer, { fetchMediaFile } from '../timeline/MediaViewer.svelte';

  type Tab = 'info' | 'media' | 'files' | 'links' | 'members';
  type MediaTab = 'media' | 'files' | 'links';

  interface Props {
    roomId: string;
    onclose: () => void;
    initialTab?: Tab;
  }

  let { roomId, onclose, initialTab = 'info' }: Props = $props();

  const TABS: { value: Tab; label: string }[] = [
    { value: 'info', label: 'Info' },
    { value: 'media', label: 'Media' },
    { value: 'files', label: 'Files' },
    { value: 'links', label: 'Links' },
    { value: 'members', label: 'Members' },
  ];

  const MEDIA_KIND: Record<MediaTab, RoomMediaKind> = {
    media: 'Media',
    files: 'Files',
    links: 'Links',
  };

  function isMediaTab(t: Tab): t is MediaTab {
    return t === 'media' || t === 'files' || t === 'links';
  }

  let tab = $state<Tab>('info');

  // Seed the tab from the prop on mount and whenever the parent changes it.
  $effect(() => {
    tab = initialTab;
  });

  const tabId = $derived(`room-info-tab-${tab}`);

  // Info tab state
  const details = $derived(roomInfo.details(roomId));
  let nameDraft = $state('');
  let topicDraft = $state('');
  let noteDraft = $state('');
  let noteDirty = $state(false);
  let noteSaving = $state(false);
  let noteSaved = $state(false);
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    nameDraft = details?.name ?? '';
    topicDraft = details?.topic ?? '';
  });

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

  async function changeAvatar() {
    const path = await pickFile();
    if (path) await roomInfo.setAvatar(roomId, path);
  }

  async function removeAvatar() {
    await roomInfo.setAvatar(roomId, null);
  }

  // Media tabs state
  const current = $derived(isMediaTab(tab) ? roomMedia.view(roomId, MEDIA_KIND[tab]) : null);
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
    const t = tab;
    if (isMediaTab(t)) untrack(() => void roomMedia.load(roomId, MEDIA_KIND[t]));
  });

  // Infinite scroll: load the next page when the sentinel at the bottom becomes visible.
  $effect(() => {
    const el = sentinel;
    const t = tab;
    if (!el || !isMediaTab(t)) return;
    const kind = MEDIA_KIND[t];
    const io = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      const view = roomMedia.view(roomId, kind);
      if (!view.loading && !view.done) void roomMedia.loadMore(roomId, kind);
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

  function onTabKey(e: KeyboardEvent) {
    const index = TABS.findIndex((t) => t.value === tab);
    let next = index;
    if (e.key === 'ArrowRight') next = (index + 1) % TABS.length;
    else if (e.key === 'ArrowLeft') next = (index - 1 + TABS.length) % TABS.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = TABS.length - 1;
    else return;
    e.preventDefault();
    tab = TABS[next].value;
    document.getElementById(`room-info-tab-${tab}`)?.focus();
  }

  // Members tab state. The roster reloads on every open (forced, so never stale) and on retry;
  // cached rows stay visible until the reply lands.
  const members = $derived(membersStore.membersOf(roomId));
  let membersLoading = $state(true);
  let membersError = $state<string | null>(null);
  let membersActionError = $state<string | null>(null);
  // Bumping this re-runs the roster load (the retry button).
  let membersAttempt = $state(0);
  // Pointer position of the last right-click on a member, used to place the profile card from the menu.
  let menuAnchor = $state({ x: 0, y: 0 });

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
    ].filter((g) => g.members.length > 0),
  );

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

  function memberMenu(m: RoomMemberDto): MenuItem[] {
    return [
      { id: 'view-profile', label: 'View profile', icon: User, onselect: () => profileCard.open(m.user_id, roomId, menuAnchor) },
      { id: 'message', label: 'Message', icon: MessageSquare, onselect: () => void messageMember(m.user_id) },
      { separator: true },
      { id: 'copy-user-id', label: 'Copy user ID', icon: Copy, onselect: () => void navigator.clipboard.writeText(m.user_id) },
    ];
  }
</script>

<section class="room-info" aria-label="Room info">
  <header class="bar">
    <h2 class="title">Room info</h2>
    <button type="button" class="icon-btn" aria-label="Close room info" title="Close room info" onclick={onclose}>
      <Icon icon={viewport.isPhone ? ChevronLeft : X} size={20} />
    </button>
  </header>

  <div class="hero">
    <Avatar userId={roomId} displayName={details?.name ?? ''} src={details?.avatar_url ?? null} size="xl" shape="rounded" />
    <div class="hero-text">
      <p class="hero-name">{details?.name || 'Unnamed room'}</p>
      {#if details?.canonical_alias}
        <p class="hero-meta">{details.canonical_alias}</p>
      {/if}
      {#if details}
        <p class="hero-meta">
          {details.encrypted ? 'Encrypted' : 'Not encrypted'} · {details.joined_members} members
        </p>
      {/if}
    </div>
  </div>

  <div class="tabs" role="tablist" aria-label="Room info sections" tabindex="-1" onkeydown={onTabKey}>
    {#each TABS as t (t.value)}
      <button
        id="room-info-tab-{t.value}"
        type="button"
        role="tab"
        class="tab"
        class:active={tab === t.value}
        aria-selected={tab === t.value}
        aria-controls="room-info-panel"
        tabindex={tab === t.value ? 0 : -1}
        onclick={() => (tab = t.value)}
      >
        {t.label}
      </button>
    {/each}
  </div>

  <div class="body" role="tabpanel" id="room-info-panel" aria-labelledby={tabId}>
    {#if tab === 'info'}
      {#if roomInfo.error(roomId)}
        <p class="notice" role="alert">{roomInfo.error(roomId)}</p>
      {/if}

      <div class="group">
        <label class="field">
          <span class="label">Name</span>
          <TextField
            maxlength={255}
            bind:value={nameDraft}
            disabled={!details?.can_set_name}
            onkeydown={(e) => {
              if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
            }}
            onblur={commitName}
          />
        </label>
        <label class="field">
          <span class="label">Topic</span>
          <TextArea rows={3} bind:value={topicDraft} disabled={!details?.can_set_topic} onblur={commitTopic} />
        </label>
        {#if roomInfo.saving(roomId)}
          <p class="status" role="status">Saving…</p>
        {/if}
      </div>

      <div class="group">
        <p class="label">Room picture</p>
        <div class="row-actions">
          {#if details?.can_set_avatar}
            <Button variant="ghost" size="sm" onclick={changeAvatar}>
              {#snippet children()}
                <Icon icon={ImagePlus} size={16} /> Change picture
              {/snippet}
            </Button>
            {#if details.avatar_url}
              <Button variant="ghost" size="sm" onclick={removeAvatar}>
                {#snippet children()}
                  <Icon icon={Trash2} size={16} /> Remove picture
                {/snippet}
              </Button>
            {/if}
          {:else}
            <p class="hint">Only room admins can change the picture.</p>
          {/if}
        </div>
      </div>

      <div class="group">
        <label class="field">
          <span class="label">Note (only visible to you)</span>
          <TextArea
            rows={4}
            value={noteDraft}
            oninput={(e) => onNoteInput(e.currentTarget.value)}
            onblur={() => void flushNote(roomId)}
          />
        </label>
        {#if noteSaving}
          <p class="status" role="status">Saving…</p>
        {:else if noteSaved}
          <p class="status" role="status">Saved</p>
        {/if}
      </div>
    {:else if isMediaTab(tab) && current}
      {#if current.error}
        <p class="notice" role="alert">{current.error}</p>
      {/if}

      {#if tab === 'media'}
        {#if current.items.length > 0}
          <div class="media-grid">
            {#each current.items as item (item.event_id)}
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
                  {#if src}
                    <img class="media-img" src={src} alt="" loading="lazy" />
                  {:else}
                    <span class="media-fallback"><Icon icon={Image} size={20} /></span>
                  {/if}
                {/await}
                {#if item.kind === 'video'}
                  <span class="play-badge" aria-hidden="true"><Icon icon={Play} size={16} /></span>
                {/if}
              </button>
            {/each}
          </div>
        {:else if current.done && !current.loading}
          <p class="empty">No shared media yet</p>
        {/if}
      {:else if tab === 'files'}
        {#if fileError}
          <p class="notice" role="alert">{fileError}</p>
        {/if}
        {#if current.items.length > 0}
          <ul class="list">
            {#each current.items as item (item.event_id)}
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
            {/each}
          </ul>
        {:else if current.done && !current.loading}
          <p class="empty">No shared files yet</p>
        {/if}
      {:else}
        {#if current.items.length > 0}
          <ul class="list">
            {#each current.items as item (item.event_id)}
              {@const url = linkOf(item)}
              <li>
                <button
                  type="button"
                  class="row"
                  aria-label="Open link {url}"
                  title={url}
                  onclick={() => void openExternal(url)}
                >
                  <span class="row-icon"><Icon icon={Link} size={20} /></span>
                  <span class="row-main">
                    <span class="row-title">{hostOf(url)}</span>
                    <span class="row-meta">{url}</span>
                    <span class="row-meta">{item.sender_display_name || item.sender} · {formatDay(item.timestamp_millis)}</span>
                  </span>
                  <Icon icon={ExternalLink} size={16} />
                </button>
              </li>
            {/each}
          </ul>
        {:else if current.done && !current.loading}
          <p class="empty">No shared links yet</p>
        {/if}
      {/if}

      {#if current.loading}
        <p class="status" role="status">Loading…</p>
      {:else if !current.done && current.items.length > 0}
        <Button variant="ghost" size="sm" onclick={() => roomMedia.loadMore(roomId, MEDIA_KIND[tab as MediaTab])}>
          {#snippet children()}Load more{/snippet}
        </Button>
      {/if}
      <div class="sentinel" bind:this={sentinel} aria-hidden="true"></div>
    {:else}
      {#snippet memberRow(m: RoomMemberDto)}
        <li>
          <button
            type="button"
            class="row"
            aria-label="Open profile for {memberLabel(m)}"
            data-user-id={m.user_id}
            onclick={(e) => openMemberProfile(e, m.user_id)}
            oncontextmenu={(e) => (menuAnchor = { x: e.clientX, y: e.clientY })}
            use:contextmenu={() => memberMenu(m)}
          >
            <Avatar userId={m.user_id} displayName={memberLabel(m)} src={m.avatar_url} size="md" shape="circle" />
            <span class="row-main">
              <UserName userId={m.user_id} displayName={memberLabel(m)} roomId={roomId} />
            </span>
          </button>
        </li>
      {/snippet}
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
          <section class="group">
            <h3 class="label">{group.title} ({group.members.length})</h3>
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

{#if viewer}
  <MediaViewer
    open={true}
    onclose={() => (viewer = null)}
    uri={viewer.uri}
    mimeType={viewer.mimeType}
    body={viewer.body}
    mediaKind={viewer.mediaKind}
  />
{/if}

<style>
  .room-info {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
    background: var(--alloy-color-surface-panel);
    color: var(--alloy-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-sm);
  }

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-space-2);
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    flex-shrink: 0;
  }

  .title {
    margin: 0;
    font-size: var(--alloy-typography-size-base);
    font-weight: 600;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: none;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-text-secondary);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-text-primary);
  }

  .hero {
    display: flex;
    align-items: center;
    gap: var(--alloy-space-3);
    padding: var(--alloy-layout-gutter);
    flex-shrink: 0;
  }

  .hero-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .hero-name {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hero-meta {
    margin: 0;
    color: var(--alloy-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }

  .tabs {
    display: flex;
    gap: var(--alloy-space-1);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    overflow-x: auto;
    flex-shrink: 0;
  }

  .tab {
    padding: var(--alloy-space-2) var(--alloy-space-2);
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--alloy-text-secondary);
    font: inherit;
    cursor: pointer;
    white-space: nowrap;
  }

  .tab.active {
    color: var(--alloy-text-primary);
    border-bottom-color: var(--alloy-color-accent-primary);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
    padding: var(--alloy-layout-gutter);
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-space-2);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-space-1);
  }

  .label {
    margin: 0;
    color: var(--alloy-text-secondary);
    font-size: var(--alloy-typography-size-xs);
    font-weight: 600;
  }

  .row-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-space-2);
    align-items: center;
  }

  .hint,
  .status,
  .empty {
    margin: 0;
    color: var(--alloy-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }

  .notice {
    margin: 0;
    padding: var(--alloy-space-2);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-xs);
  }

  .media-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--alloy-space-1);
  }

  .media-cell {
    position: relative;
    aspect-ratio: 1;
    padding: 0;
    border: none;
    border-radius: var(--alloy-radii-md);
    overflow: hidden;
    background: var(--alloy-color-surface-subtle);
    cursor: pointer;
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
    color: var(--alloy-text-muted);
  }

  .play-badge {
    position: absolute;
    right: var(--alloy-space-1);
    bottom: var(--alloy-space-1);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-text-primary);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-space-1);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-space-3);
    width: 100%;
    padding: var(--alloy-space-2);
    border: none;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .row:hover:not(:disabled) {
    background: var(--alloy-color-surface-hover);
  }

  .row:disabled {
    cursor: progress;
  }

  .row-icon {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--alloy-text-secondary);
  }

  .row-main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }

  .row-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-color-text-link);
  }

  .row-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }

  .sentinel {
    height: 1px;
  }

  /* Phone: a full-screen page with a back arrow, not a floating card. */
  @media (max-width: 899px) {
    .room-info {
      position: fixed;
      inset: 0;
      z-index: 50;
      box-sizing: border-box;
      padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
        var(--alloy-safe-area-left);
      font-size: var(--alloy-typography-size-lg);
    }
    .bar {
      height: var(--alloy-layout-header-height);
      padding: 0 var(--alloy-spacing-scale-2);
      justify-content: flex-start;
    }
    .title {
      flex: 1;
      min-width: 0;
      font-size: var(--alloy-typography-size-lg);
    }
    .icon-btn {
      order: -1;
      width: 44px;
      height: 44px;
    }
    .icon-btn:hover {
      background: transparent;
      color: var(--alloy-text-secondary);
    }
    .hero-meta,
    .hint,
    .status,
    .empty,
    .label,
    .row-meta {
      font-size: var(--alloy-typography-size-sm);
    }
    .tab {
      min-height: 44px;
      padding: 0 var(--alloy-spacing-scale-3);
      font-size: var(--alloy-typography-size-lg);
    }
    .body {
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
    }
    .body :global(button) {
      min-height: 44px;
    }
    .row-actions :global(button) {
      min-height: 44px;
    }
    .row {
      min-height: 56px;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    .row:hover:not(:disabled) {
      background: transparent;
    }
    .row:active:not(:disabled) {
      background: var(--alloy-color-surface-hover);
    }
    .row-title {
      font-size: var(--alloy-typography-size-lg);
    }
  }
</style>
