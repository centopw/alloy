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
  }
  let { room, onback }: Props = $props();

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
    <button
      type="button"
      class="identity"
      aria-label={subtitle
        ? `${t('chat-header-room-info', { room: title })}, ${subtitle}`
        : t('chat-header-room-info', { room: title })}
      data-tooltip={t('chat-header-room-info-2')}
      onclick={() => ui.toggleRoomInfo()}
    >
      <span class="avatar-slot">{@render avatarEl()}</span>
      <!-- The identity is one block: the name, then the one subtitle line directly under it, so the
           two lines share a left edge on every viewport. -->
      <span class="identity-text">
        <span class="title-line">
          <span class="title">{title}</span>
          {#if !phone}
            <span class="net" aria-label={networkName(room.network)} title={networkName(room.network)}>{networkBadge(room.network)}</span>
          {/if}
        </span>
        {#if subtitle}
          <!-- A topic reads topic over forum; a forum shows its topic count instead. -->
          <span class="sub">{subtitle}</span>
        {/if}
      </span>
    </button>
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
  /* One row, one rail: the header starts where the conversation's bubbles start, so the identity,
     the pinned bar and the composer all share a left edge. */
  .head {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    min-width: 0;
    height: var(--alloy-layout-header-height);
    padding: 0 var(--conversation-rail, var(--alloy-layout-gutter)) 0
      max(var(--conversation-rail, var(--alloy-layout-gutter)), var(--chat-header-lead, 0px));
    border-bottom: 1px solid var(--alloy-chat-rim);
    background: transparent;
  }
  .heading {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
    margin: 0;
    font-size: inherit;
    font-weight: inherit;
  }
  /* The identity is a block: avatar column and a two-line text column. It hugs its content so the
     rest of the `h1` stays a drag region and the window can still be moved from its own header. */
  .identity {
    display: flex;
    flex: 0 1 auto;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
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
  .identity-text {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
  }
  .identity:hover .title {
    text-decoration: underline;
  }
  .identity:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .title-line {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    line-height: 20px;
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
  /* The one subtitle line: the topic's forum, or a forum's topic count. It sits directly under the
     title inside the same text column, so it shares the title's left edge on every viewport. */
  .sub {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-tight);
    color: var(--alloy-color-text-muted);
    text-align: left;
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
  /* Phones: same 56px row, but every control keeps the 44px touch target the mobile guardrail
     requires. The bar grows past 56px when the text scale is enlarged, so a 200% text/zoom check
     never clips the room name into a fixed height (A11Y-03). */
  @media (max-width: 899px) {
    .head {
      height: auto;
      min-height: var(--alloy-layout-header-height);
      padding: var(--alloy-spacing-scale-1) var(--conversation-rail, var(--alloy-layout-gutter));
    }
    .icon,
    .back {
      width: 44px;
      height: 44px;
    }
  }
</style>
