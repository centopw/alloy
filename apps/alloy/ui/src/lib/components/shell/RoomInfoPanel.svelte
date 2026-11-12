<script lang="ts">
  import { untrack } from 'svelte';
  import { Icon, X, ChevronLeft, File, ExternalLink, Download } from '$lib/icons';
  import { formatBytes, formatDay } from '$lib/content/format';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { openExternal } from '$lib/platform/links';
  import { localFileUrl } from '$lib/platform/files';
  import { pushBack } from '$lib/platform/backstack';
  import { client } from '$lib/ipc';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { roomMedia } from '$lib/stores/roomMedia.svelte';
  import { membersStore, memberLabel } from '$lib/stores/members.svelte';
  import { networkName, roomsStore } from '$lib/stores/rooms.svelte';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import type { RoomInfoTab } from '$lib/stores/ui.svelte';
  import type { MediaSource, RoomMediaItem, RoomMediaKind } from '$lib/generated/protocol';
  import type { RoomMemberDto } from '$lib/generated';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { Avatar, Button, SegmentedControl, TextArea } from '../common';

  import { t } from '$lib/i18n';

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
    media: t('room-info-panel-shared-media'),
    files: t('room-info-panel-files-and-documents'),
    links: t('room-info-panel-shared-links'),
  };

  /** Rows shown on the overview before "View all" opens the full list. */
  const PREVIEW = { media: 6, files: 3, links: 3, members: 5 } as const;

  /** Shared-media tabs in display order. */
  const SHARED_TABS: { value: MediaView; label: string }[] = [
    { value: 'media', label: t('room-info-panel-media') },
    { value: 'links', label: t('room-info-panel-links') },
    { value: 'files', label: t('room-info-panel-files') },
  ];

  const EMPTY_TEXT: Record<MediaView, string> = {
    media: t('room-info-panel-no-shared-media-yet'),
    files: t('room-info-panel-no-shared-files-yet'),
    links: t('room-info-panel-no-shared-links-yet'),
  };

  function isMediaView(v: View): v is MediaView {
    return v === 'media' || v === 'files' || v === 'links';
  }

  function viewAll(count: number, done: boolean): string {
    return done ? t('room-info-panel-view-all-count', { count }) : t('room-info-panel-view-all');
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

  const title = $derived(
    view === 'overview'
      ? t('room-info-panel-details')
      : view === 'members'
        ? t('room-info-panel-members-2')
        : MEDIA_TITLE[view]
  );

  const details = $derived(roomInfo.details(roomId));
  const room = $derived(roomsStore.rooms.find((r) => r.id === roomId) ?? null);
  const account = $derived(
    room ? (roomsStore.accounts.find((a) => a.id === room.account_id) ?? null) : null
  );

  // Thumbnails resolved per item; a failed picture keeps its kind icon.
  let thumbSrc = $state<Record<string, string>>({});
  const thumbRequested = new Set<string>();

  async function resolveMedia(source: MediaSource, kind: 'thumbnail' | 'file', mime: string | null) {
    const res = await client.invoke({
      action: 'GetMedia',
      params: { source, kind, mime_type: mime },
    });
    if (res.type !== 'Media') throw new Error(`unexpected response: ${res.type}`);
    return await localFileUrl(res.payload.path);
  }

  function ensureThumb(item: RoomMediaItem) {
    if (!item.source || item.kind === 'link' || item.kind === 'audio' || thumbRequested.has(item.event_id)) return;
    thumbRequested.add(item.event_id);
    void resolveMedia(item.source, 'thumbnail', item.kind === 'video' ? 'image/jpeg' : item.mime_type)
      .then((url) => (thumbSrc = { ...thumbSrc, [item.event_id]: url }))
      .catch(() => {
        /* the tile keeps its kind icon */
      });
  }

  // Note state.
  let noteDraft = $state('');
  let noteDirty = $state(false);
  let noteSaving = $state(false);
  let noteSaved = $state(false);
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

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

  // Media state. The overview previews all three kinds; a sub-view loads only its own kind.
  const mediaState = $derived({
    media: roomMedia.view(roomId, 'Media'),
    files: roomMedia.view(roomId, 'Files'),
    links: roomMedia.view(roomId, 'Links'),
  });
  let sharedTab = $state<MediaView>('media');
  const sharedHasItems = $derived(SHARED_TABS.some(({ value }) => mediaState[value].items.length > 0));
  const current = $derived(isMediaView(view) ? mediaState[view] : null);
  /** Empty-state copy for the media sub-view that is open. */
  const emptyText = $derived(isMediaView(view) ? EMPTY_TEXT[view] : '');
  let sentinel = $state<HTMLElement | null>(null);
  let busyItem = $state<string | null>(null);
  let mediaError = $state<string | null>(null);

  $effect(() => {
    const id = roomId;
    const v = view;
    const kinds: MediaView[] = v === 'overview' ? ['media', 'files', 'links'] : isMediaView(v) ? [v] : [];
    untrack(() => {
      for (const k of kinds) void roomMedia.load(id, MEDIA_KIND[k]);
    });
  });

  // A picture sent while the panel is open has to appear without reopening it. The lists are read
  // from the store once, so watch the room's newest event and refresh the visible kind; the delay
  // collapses a burst into one read, and a failed refresh keeps the list it already has.
  $effect(() => {
    const id = roomId;
    const v = view;
    const newest = roomsStore.rooms.find((r) => r.id === id)?.latest_event?.timestamp_millis ?? null;
    void newest;
    if (newest === null) return;
    const timer = window.setTimeout(() => {
      const kinds: MediaView[] = v === 'overview' ? ['media', 'files', 'links'] : isMediaView(v) ? [v] : [];
      for (const k of kinds) void roomMedia.refresh(id, MEDIA_KIND[k]);
    }, 350);
    return () => window.clearTimeout(timer);
  });

  // Resolve a thumbnail for every picture tile the current view shows.
  $effect(() => {
    const v = view;
    const kinds: MediaView[] = v === 'overview' ? ['media', 'files', 'links'] : isMediaView(v) ? [v] : [];
    for (const k of kinds) for (const item of mediaState[k].items) ensureThumb(item);
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

  /** Retries the kind the media sub-view is showing. */
  function reloadCurrent() {
    if (isMediaView(view)) void roomMedia.load(roomId, MEDIA_KIND[view]);
  }

  async function openMedia(item: RoomMediaItem) {
    if (!item.source) {
      const link = linkOf(item);
      await openExternal(link).catch(() => undefined);
      return;
    }
    busyItem = item.event_id;
    mediaError = null;
    try {
      const url = await resolveMedia(item.source, 'file', item.mime_type ?? null);
      await openExternal(url);
    } catch (e) {
      mediaError = errorMessage(e);
    } finally {
      busyItem = null;
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

  // Members. The roster reloads on every open (forced, so never stale) and on retry.
  const members = $derived(membersStore.membersOf(roomId));
  let membersLoading = $state(true);
  let membersError = $state<string | null>(null);
  let membersAttempt = $state(0);

  function byName(a: RoomMemberDto, b: RoomMemberDto): number {
    return memberLabel(a).localeCompare(memberLabel(b));
  }

  const groups = $derived.by(() => {
    const byRole = new Map<string, RoomMemberDto[]>();
    for (const m of members) {
      const key = m.role ?? '';
      const bucket = byRole.get(key);
      if (bucket) bucket.push(m);
      else byRole.set(key, [m]);
    }
    const rank = (role: string) =>
      role === 'owner' ? 0 : role === 'admin' || role === 'moderator' ? 1 : 2;
    return [...byRole.entries()]
      .sort(([a], [b]) => rank(a) - rank(b) || a.localeCompare(b))
      .map(([role, list]) => ({
        title: role === '' ? t('room-info-panel-member') : role.charAt(0).toUpperCase() + role.slice(1),
        members: [...list].sort(byName),
      }));
  });

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
    profileCard.open(userId, roomId, at);
  }

  let panel = $state<HTMLElement | null>(null);

  /** Centre of a member's row. The row stays mounted after its context menu closes, so it anchors the profile card. */
  function memberRowCentre(userId: string): { x: number; y: number } | null {
    const row = panel?.querySelector<HTMLElement>(`[data-user-id="${CSS.escape(userId)}"]`);
    if (!row) return null;
    const r = row.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }

  function memberItems(m: RoomMemberDto): MenuItem[] {
    return [
      {
        id: 'profile',
        label: t('room-info-panel-view-profile'),
        onselect: () => {
          const at = memberRowCentre(m.user_id);
          if (at) profileCard.open(m.user_id, roomId, at);
        },
      },
      {
        id: 'copy-id',
        label: t('user-profile-card-copy-user-id'),
        onselect: () => void navigator.clipboard.writeText(m.user_id).catch(() => undefined),
      },
    ];
  }

  function previewRows(kind: MediaView): RoomMediaItem[] {
    return mediaState[kind].items.slice(0, PREVIEW[kind]);
  }

  async function copyRoomId() {
    await navigator.clipboard.writeText(roomId).catch(() => undefined);
  }

</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside class="panel" bind:this={panel} aria-label={t('room-info-panel-room-info')} onkeydown={(e) => e.key === 'Escape' && onclose()}>
  <header class="bar">
    <button
      type="button"
      class="icon-btn"
      aria-label={view === 'overview' ? t('room-info-panel-close-room-info') : t('room-info-panel-back-to-details')}
      title={view === 'overview' ? t('room-info-panel-close-room-info') : t('room-info-panel-back-to-details')}
      onclick={() => (view === 'overview' ? onclose() : (view = 'overview'))}
    >
      <Icon icon={view === 'overview' ? X : ChevronLeft} size={20} />
    </button>
    <h2 class="title">{title}</h2>
    {#if onsearch && view === 'overview'}
      <Button variant="ghost" size="sm" onclick={onsearch}>{t('chat-header-search-in-room')}</Button>
    {/if}
  </header>

  <div class="body">
    {#if view === 'overview'}
      <section class="block" aria-label={t('room-info-panel-about')}>
        <dl class="facts">
          <div class="fact"><dt>{t('chat-info-network')}</dt><dd>{details ? networkName(details.network) : room ? networkName(room.network) : ''}</dd></div>
          {#if account}
            <div class="fact"><dt>{t('chat-info-account')}</dt><dd>{account.display_name ?? networkName(account.network)}</dd></div>
          {/if}
          <div class="fact"><dt>{t('chat-info-kind')}</dt><dd>{details ? t(`chat-info-kind-${details.kind}`) : ''}</dd></div>
          <div class="fact"><dt>{t('chat-info-members')}</dt><dd>{details?.member_count ?? members.length}</dd></div>
        </dl>
        <Button variant="ghost" size="sm" onclick={() => void copyRoomId()}>{t('room-menu-copy-room-id')}</Button>
      </section>

      <section class="block" aria-label={t('room-info-panel-private-note')}>
        <h3>{t('room-info-panel-private-note')}</h3>
        <p class="hint">{t('room-info-panel-a-note-only-you-can-see')}</p>
        <TextArea
          value={noteDraft}
          placeholder={t('room-info-panel-edit-private-note')}
          aria-label={t('room-info-panel-private-note')}
          oninput={(e: Event) => onNoteInput((e.currentTarget as HTMLTextAreaElement).value)}
        />
        <p class="hint" role="status">
          {#if noteSaving}{t('room-info-panel-saving')}{:else if noteSaved}{t('room-info-panel-saved')}{/if}
        </p>
        {#if roomInfo.error(roomId)}<p class="notice" role="alert">{roomInfo.error(roomId)}</p>{/if}
      </section>

      <section class="block" aria-label={t('room-info-panel-shared-content')}>
        <h3>{t('room-info-panel-shared-content')}</h3>
        {#if !sharedHasItems}
          <p class="hint">{t('room-info-panel-no-shared-media-yet')}</p>
        {:else}
          <div class="tabs">
            <SegmentedControl
              label={t('room-info-panel-shared-content')}
              options={SHARED_TABS.map((tab) => ({ value: tab.value, label: tab.label }))}
              value={sharedTab}
              onchange={(v) => (sharedTab = v as MediaView)}
            />
          </div>
          <ul class="tiles">
            {#each previewRows(sharedTab) as item (item.event_id)}
              <li>
                <button type="button" class="tile" disabled={busyItem === item.event_id} onclick={() => void openMedia(item)} aria-label={t('room-info-panel-open-attachment', { name: item.body })}>
                  {#if thumbSrc[item.event_id]}
                    <img src={thumbSrc[item.event_id]} alt="" loading="lazy" />
                  {:else}
                    <span class="tile-icon"><Icon icon={item.kind === 'link' ? ExternalLink : File} size={20} /></span>
                  {/if}
                  <span class="tile-name">{item.kind === 'link' ? hostOf(linkOf(item)) : item.body}</span>
                  <span class="tile-meta">{formatDay(item.timestamp_millis)}{#if item.size_bytes} · {formatBytes(item.size_bytes)}{/if}</span>
                </button>
              </li>
            {/each}
          </ul>
          <Button variant="ghost" size="sm" onclick={() => (view = sharedTab)}>
            {viewAll(mediaState[sharedTab].items.length, mediaState[sharedTab].done)}
          </Button>
        {/if}
        {#if mediaError}<p class="notice" role="alert">{mediaError}</p>{/if}
      </section>

      <section class="block" aria-label={t('room-info-panel-members-2')}>
        <h3>{t('room-info-panel-members-2')}</h3>
        {#each groups.slice(0, 1) as group (group.title)}
          <ul class="members">
            {#each group.members.slice(0, PREVIEW.members) as m (m.user_id)}
              <li>
                <button type="button" class="member" data-user-id={m.user_id} onclick={(e) => openMemberProfile(e, m.user_id)} use:contextmenu={{ items: () => memberItems(m), options: () => ({ ariaLabel: t('room-info-panel-open-profile', { name: memberLabel(m) }) }) }}>
                  <Avatar userId={m.user_id} displayName={m.display_name} size="sm" />
                  <span class="member-name">{memberLabel(m)}</span>
                  {#if m.role}<span class="role">{m.role}</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/each}
        <Button variant="ghost" size="sm" onclick={() => (view = 'members')}>
          {t('room-info-panel-view-all')}
        </Button>
      </section>
    {:else if view === 'members'}
      {#if membersError}
        <div class="notice" role="alert">
          <p>{t('room-info-panel-could-not-load-members')} {membersError}</p>
          <Button variant="ghost" size="sm" onclick={() => (membersAttempt += 1)}>{t('room-info-panel-try-again')}</Button>
        </div>
      {:else if membersLoading && members.length === 0}
        <p class="hint" role="status">{t('room-info-panel-loading-members')}</p>
      {:else}
        {#each groups as group (group.title)}
          <section class="block" aria-label={group.title}>
            <h3>{group.title}<span class="section-count">{group.members.length}</span></h3>
            <ul class="members">
              {#each group.members as m (m.user_id)}
                <li>
                  <button type="button" class="member" data-user-id={m.user_id} onclick={(e) => openMemberProfile(e, m.user_id)} use:contextmenu={{ items: () => memberItems(m), options: () => ({ ariaLabel: t('room-info-panel-open-profile', { name: memberLabel(m) }) }) }}>
                    <Avatar userId={m.user_id} displayName={m.display_name} size="sm" />
                    <span class="member-name">{memberLabel(m)}</span>
                    {#if m.role}<span class="role">{m.role}</span>{/if}
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      {/if}
    {:else if current}
      {#if current.error}
        <div class="notice" role="alert">
          <p>{current.error}</p>
          <Button variant="ghost" size="sm" onclick={reloadCurrent}>{t('room-info-panel-try-again')}</Button>
        </div>
      {/if}
      {#if current.items.length === 0 && !current.loading}
        <p class="hint">{emptyText}</p>
      {:else}
        <ul class="tiles full">
          {#each current.items as item (item.event_id)}
            <li>
              <button type="button" class="tile" disabled={busyItem === item.event_id} onclick={() => void openMedia(item)} aria-label={t('room-info-panel-open-attachment', { name: item.body })}>
                {#if thumbSrc[item.event_id]}
                  <img src={thumbSrc[item.event_id]} alt="" loading="lazy" />
                {:else}
                  <span class="tile-icon"><Icon icon={item.kind === 'link' ? ExternalLink : item.kind === 'audio' ? Download : File} size={20} /></span>
                {/if}
                <span class="tile-name">{item.kind === 'link' ? hostOf(linkOf(item)) : item.body}</span>
                <span class="tile-meta">{formatDay(item.timestamp_millis)}{#if item.size_bytes} · {formatBytes(item.size_bytes)}{/if}</span>
              </button>
            </li>
          {/each}
        </ul>
        {#if current.loading}<p class="hint" role="status">{t('room-info-panel-loading')}</p>{/if}
        <div bind:this={sentinel}></div>
      {/if}
      {#if mediaError}<p class="notice" role="alert">{mediaError}</p>{/if}
    {/if}
  </div>
</aside>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    color: var(--alloy-color-text-primary);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-glass-rim);
    flex-shrink: 0;
  }
  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
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
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .block h3 {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }
  .section-count {
    padding: 1px 6px; /* token-audit: kbd chip inset, set by the 10px meta text */
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-control);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-secondary);
  }
  .facts {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    margin: 0;
  }
  .fact {
    display: flex;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-sm);
  }
  .fact dt {
    color: var(--alloy-color-text-secondary);
  }
  .fact dd {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    margin: 0;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
  }
  .notice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    padding: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-xs);
  }
  .notice p {
    margin: 0;
  }
  .tabs {
    display: flex;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .tiles.full {
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    min-width: 0;
    padding: var(--alloy-spacing-scale-1);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-secondary);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .tile img {
    width: 100%;
    aspect-ratio: 4 / 3;
    object-fit: cover;
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-hover);
  }
  .tile-icon {
    display: grid;
    place-items: center;
    width: 100%;
    aspect-ratio: 4 / 3;
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-muted);
  }
  .tile-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-primary);
  }
  .tile-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .members {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .member {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-height: 40px;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .member:hover {
    background: var(--alloy-glass-control);
  }
  .member-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .role {
    flex: none;
    padding: 0 6px; /* token-audit: role chip inset, 10px meta text */
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
    color: var(--alloy-color-text-muted);
    font-size: 10px;
    letter-spacing: 0;
    text-transform: none;
  }
  .member,
  .tile {
    transition: background var(--alloy-motion-duration-fast) ease;
  }
  .tile:hover {
    background: var(--alloy-glass-control-hover);
  }
  @media (max-width: 899px) {
    .member {
      min-height: 44px;
    }
    .icon-btn {
      width: 44px;
      height: 44px;
    }
  }
</style>
