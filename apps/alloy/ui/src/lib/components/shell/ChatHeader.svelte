<script lang="ts">
  import type { RoomSummary } from '$lib/generated/protocol';
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { CheckCheck, ChevronLeft, Info, Search, Users } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { networkBadge, networkName, roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { resolveAvatar } from '$lib/stores/avatars.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { copyItems } from './roomMenus';
  import { forumTopicCount } from './forumTopics';

  import { t } from '$lib/i18n';
  interface Props {
    room: RoomSummary;
    onback?: () => void;
    /** Opens a forum's topic list: the breadcrumb back from a topic to all its siblings. */
    onopenforum?: (roomId: string) => void;
  }
  let { room, onback, onopenforum }: Props = $props();

  const roomId = $derived(room.id);
  const title = $derived(roomTitle(room));
  const phone = $derived(viewport.isPhone);
  const details = $derived(membersStore.detailsOf(roomId));
  const memberCount = $derived(
    details?.member_count ?? (membersStore.membersOf(roomId).length || null)
  );
  // Phones have no inline subtitle on the title row, so the second line carries the member count.
  // A topic shows its forum as the subtitle, so the header reads as a breadcrumb: topic over forum.
  const forum = $derived(room.parent_id ? roomsStore.rooms.find((r) => r.id === room.parent_id) ?? null : null);
  // A forum is a container with no messages and no member list of its own: its count is its topics.
  const topicCount = $derived(room.is_forum ? forumTopicCount(roomsStore.rooms, roomId) : 0);
  const subtitle = $derived(
    forum
      ? roomTitle(forum)
      : room.is_forum
        ? t('forum-topic-count', { count: topicCount })
        : memberCount === null
          ? null
          : t('chat-header-member-count', { count: memberCount })
  );
  const searchShortcut = shortcutLabel('F');
  let avatarSrc = $state<string | null>(null);

  $effect(() => {
    const source = room.avatar;
    let cancelled = false;
    avatarSrc = null;
    void resolveAvatar(source, 32).then((url) => {
      if (!cancelled) avatarSrc = url;
    });
    return () => {
      cancelled = true;
    };
  });

  function headerMenu(): MenuItem[] {
    // A forum holds no messages, so searching inside it would always come back empty.
    const search: MenuItem = {
      id: 'search',
      label: t('chat-header-search-in-room'),
      icon: Search,
      shortcut: searchShortcut,
      onselect: () => ui.openRoomSearch(),
    };
    return [
      { id: 'details', label: t('common-room-details'), icon: Info, onselect: () => ui.openRoomInfo('info') },
      ...(room.is_forum ? [] : [search]),
      { separator: true },
      ...copyItems(room),
      { separator: true },
      {
        id: 'mark-read',
        label: t('common-mark-as-read'),
        icon: CheckCheck,
        disabled: roomsStore.unreadOf(room) === 0,
        onselect: () => roomsStore.markRead(roomId),
      },
    ];
  }

  // The member count and kind live in the room details, which may not have loaded yet. A forum has
  // no member list, only topics, so there is nothing to load for it.
  $effect(() => {
    if (room.is_forum) return;
    void membersStore.loadDetails(roomId).catch(() => {
      /* the header simply shows no member count */
    });
  });
</script>

{#snippet avatarEl()}
  <Avatar userId={roomId} displayName={title} src={avatarSrc} size="sm" shape="rounded" />
{/snippet}

<header class="head" data-tauri-drag-region use:contextmenu={headerMenu}>
  <button type="button" class="icon back" aria-label={t('chat-header-back-to-rooms')} onclick={() => onback?.()} use:contextmenu={() => [{ id: 'back', label: t('chat-header-back-to-rooms'), icon: ChevronLeft, onselect: () => onback?.() }]}>
    <Icon icon={ChevronLeft} size={20} />
  </button>
  <h1 class="heading" tabindex="-1" data-chat-entry data-tauri-drag-region>
    <div class="identity" data-tauri-drag-region>
      <button type="button" class="info-btn" aria-label={t('chat-header-room-info', { room: title })} data-tooltip={t('chat-header-room-info-2')} onclick={() => ui.toggleRoomInfo()}>
        <span class="avatar-slot">{@render avatarEl()}</span>
        <span class="title">{title}</span>
        {#if !phone}
          <span class="net" aria-label={networkName(room.network)} title={networkName(room.network)}>{networkBadge(room.network)}</span>
        {/if}
      </button>
      {#if subtitle}
        {#if forum}
          <!-- The breadcrumb back from a topic to all of its siblings: one click opens the forum. -->
          <button
            type="button"
            class="sub forum-link"
            aria-label={t('chat-header-show-forum-topics', { name: subtitle })}
            onclick={() => onopenforum?.(forum.id)}
          >{subtitle}</button>
        {:else}
          <span class="sub">{subtitle}</span>
        {/if}
      {/if}
    </div>
  </h1>
  <div class="actions">
    <button
      type="button"
      class="icon"
      use:contextmenu={() => [{ id: 'details', label: t('common-room-details'), icon: Info, onselect: () => ui.toggleRoomInfo('info') }]}
      aria-label={t('chat-header-room-info-button')}
      aria-pressed={ui.roomInfoOpen && ui.roomInfoTab === 'info'}
      onclick={() => ui.toggleRoomInfo('info')}
    >
      <Icon icon={Info} size={20} />
    </button>
    {#if !phone}
    <button
      type="button"
      class="icon"
      use:contextmenu={() => [{ id: 'members', label: t('chat-header-show-members'), icon: Users, onselect: () => ui.toggleRoomInfo('members') }]}
      aria-label={t('chat-header-member-list')}
      aria-pressed={ui.roomInfoOpen && ui.roomInfoTab === 'members'}
      onclick={() => ui.toggleRoomInfo('members')}
    >
      <Icon icon={Users} size={20} />
    </button>
    {/if}
    {#if !room.is_forum}
      <button
        type="button"
        class="icon"
        use:contextmenu={() => [{ id: 'search', label: t('chat-header-search-in-room'), icon: Search, onselect: () => ui.openRoomSearch() }]}
        aria-label={t('chat-header-search-in-room')}
        data-tooltip={t('chat-header-search-in-room')}
        data-shortcut={searchShortcut}
        onclick={() => ui.openRoomSearch()}
      >
        <Icon icon={Search} size={20} />
      </button>
    {/if}
  </div>
</header>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    min-width: 0;
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter) 0 max(var(--alloy-layout-gutter), var(--chat-header-lead, 0px));
    border-bottom: 1px solid var(--alloy-chat-rim);
    background: transparent;
  }
  .heading {
    flex: 1 1 0;
    min-width: 0;
    margin: 0;
    font-size: inherit;
    font-weight: inherit;
  }
  .identity {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-width: 0;
    height: 100%;
  }
  .info-btn {
    display: flex;
    flex: 0 1 auto;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .avatar-slot {
    display: inline-flex;
    flex: none;
  }
  .info-btn:hover .title {
    text-decoration: underline;
  }
  .info-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .title {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.01em;
    color: var(--alloy-color-text-primary);
  }
  /* The network a chat belongs to, as text (ADR 0001: no decorative icons). */
  .net {
    flex: none;
    padding: 0 var(--alloy-spacing-scale-1);
    border: 1px solid var(--alloy-chat-rim);
    border-radius: var(--alloy-radii-sm);
    color: var(--alloy-color-text-muted);
    font-size: 10px;
    font-weight: var(--alloy-typography-weight-semibold);
    line-height: 15px;
  }
  /* The second line: the topic's forum (a button back to all topics) or a forum's topic count. */
  .sub {
    flex: 1 1 auto;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    text-align: left;
  }
  /* Desktop keeps the breadcrumb on one line, behind the dot the old static subtitle used. */
  .identity > .sub::before {
    content: '';
    display: inline-block;
    width: 3px;
    height: 3px;
    margin-right: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
    background: color-mix(in srgb, var(--alloy-color-text-muted) 70%, transparent);
    vertical-align: middle;
  }
  .forum-link {
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: inherit;
    font-family: inherit;
    cursor: pointer;
  }
  .forum-link:hover {
    color: var(--alloy-color-text-primary);
    text-decoration: underline;
  }
  .forum-link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    margin-left: auto;
    flex: none;
  }
  .icon {
    flex: none;
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
  .icon:hover {
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-primary);
  }
  .icon[aria-pressed='true'] {
    color: var(--alloy-color-accent-primary);
  }
  .icon:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .back {
    display: none;
  }
  /* Narrow layouts show the room list as a separate pane, so the header needs a way back. */
  @media (max-width: 899px) {
    .back {
      display: grid;
    }
  }
  /* Phones: 56px minimum bar, 44px targets with 8px between them, a two-line identity. The bar
     grows past 56px when the text scale is enlarged, so a 200% text/zoom check never clips the
     room name into the fixed height (A11Y-03). */
  @media (max-width: 899px) {
    .head {
      height: auto;
      min-height: 56px;
      padding: 0 var(--alloy-spacing-scale-2);
    }
    .actions {
      gap: var(--alloy-spacing-scale-2);
    }
    .icon {
      width: 44px;
      height: 44px;
    }
    .info-btn {
      flex: 1 1 100%;
      gap: var(--alloy-spacing-scale-3);
    }
    /* Two lines where there is a second one: the title, then the subtitle under its text. The
       subtitle stays a separate control, so it wraps instead of living inside the info button. */
    .identity {
      flex-wrap: wrap;
      row-gap: 0;
    }
    .identity > .sub::before {
      display: none;
    }
    .sub {
      flex: 1 1 100%;
      max-width: 100%;
      padding-left: calc(28px + var(--alloy-spacing-scale-3));
      font-size: var(--alloy-typography-size-xs);
      color: var(--alloy-color-text-secondary);
    }
  }
</style>
