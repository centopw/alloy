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
  const subtitle = $derived(
    memberCount === null ? null : t('chat-header-member-count', { count: memberCount })
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
    return [
      { id: 'details', label: t('common-room-details'), icon: Info, onselect: () => ui.openRoomInfo('info') },
      {
        id: 'search',
        label: t('chat-header-search-in-room'),
        icon: Search,
        shortcut: searchShortcut,
        onselect: () => ui.openRoomSearch(),
      },
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

  // The member count and kind live in the room details, which may not have loaded yet.
  $effect(() => {
    void membersStore.loadDetails(roomId).catch(() => {
      /* the header simply shows no member count */
    });
  });
</script>

{#snippet avatarEl()}
  <Avatar userId={roomId} displayName={title} src={avatarSrc} size="sm" shape="rounded" />
{/snippet}

<header class="head" use:contextmenu={headerMenu}>
  <button type="button" class="icon back" aria-label={t('chat-header-back-to-rooms')} onclick={() => onback?.()} use:contextmenu={() => [{ id: 'back', label: t('chat-header-back-to-rooms'), icon: ChevronLeft, onselect: () => onback?.() }]}>
    <Icon icon={ChevronLeft} size={20} />
  </button>
  <h1 class="heading" tabindex="-1" data-chat-entry>
    <div class="identity">
      <button type="button" class="info-btn" aria-label={t('chat-header-room-info', { room: title })} data-tooltip={t('chat-header-room-info-2')} onclick={() => ui.toggleRoomInfo()}>
        <span class="avatar-slot">{@render avatarEl()}</span>
        {#if phone}
          <span class="stack">
            <span class="title">{title}</span>
            {#if subtitle}
              <span class="sub">{subtitle}</span>
            {/if}
          </span>
        {:else}
          <span class="title">{title}</span>
          <span class="net" aria-label={networkName(room.network)} title={networkName(room.network)}>{networkBadge(room.network)}</span>
          {#if subtitle}
            <span class="divider" aria-hidden="true"></span>
            <span class="topic">{subtitle}</span>
          {/if}
        {/if}
      </button>
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
    padding: 0 var(--alloy-layout-gutter);
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
    flex: 1 1 0;
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
  .divider {
    flex-shrink: 0;
    width: 3px;
    height: 3px;
    border-radius: var(--alloy-radii-full);
    background: color-mix(in srgb, var(--alloy-color-text-muted) 70%, transparent);
  }
  .topic {
    flex: 1 1 0;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
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
      gap: var(--alloy-spacing-scale-3);
    }
    .stack {
      display: flex;
      flex: 1 1 auto;
      min-width: 0;
      overflow: hidden;
      flex-direction: column;
      justify-content: center;
      align-items: stretch;
    }
    .sub {
      max-width: 100%;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-size: var(--alloy-typography-size-xs);
      color: var(--alloy-color-text-secondary);
    }
  }
</style>
