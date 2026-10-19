<script lang="ts">
  import type { RoomSummary } from '$lib/generated/protocol';
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { CheckCheck, ChevronLeft, Copy, Info, Link, Search, Users } from '$lib/icons';
  import { copyText } from '$lib/platform/clipboard';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';

  interface Props {
    room: RoomSummary;
    onback?: () => void;
    onsearch?: () => void;
  }
  let { room, onback, onsearch }: Props = $props();

  const roomId = $derived(room.id);
  const title = $derived(roomTitle(room));
  const topic = $derived(membersStore.detailsOf(roomId)?.topic?.trim() || null);
  const phone = $derived(viewport.isPhone);
  const members = $derived(membersStore.detailsOf(roomId)?.joined_members ?? null);
  // Phones have no inline topic divider, so the second line carries the topic, else the member count.
  const subtitle = $derived(
    topic ?? (members === null ? null : `${members} ${members === 1 ? 'member' : 'members'}`),
  );
  const searchShortcut = shortcutLabel('K');

  function copyQuietly(text: string) {
    copyText(text).catch(() => {
      /* the clipboard refused the write; the menu has no other channel to report it */
    });
  }

  function headerMenu(): MenuItem[] {
    return [
      { id: 'details', label: 'Room details', icon: Info, onselect: () => ui.openRoomInfo('info') },
      {
        id: 'search',
        label: 'Search in room',
        icon: Search,
        shortcut: searchShortcut,
        disabled: !onsearch,
        onselect: () => onsearch?.(),
      },
      { separator: true },
      {
        id: 'copy-link',
        label: 'Copy room link',
        icon: Link,
        onselect: () => copyQuietly(`https://matrix.to/#/${roomId}`),
      },
      { id: 'copy-id', label: 'Copy room ID', icon: Copy, onselect: () => copyQuietly(roomId) },
      { separator: true },
      {
        id: 'mark-read',
        label: 'Mark as read',
        icon: CheckCheck,
        disabled: roomsStore.unreadOf(room) === 0,
        onselect: () => roomsStore.markRead(roomId),
      },
    ];
  }

  // The topic lives in the room details, which may not have loaded yet.
  $effect(() => {
    void membersStore.loadDetails(roomId).catch(() => {
      /* the header simply shows no topic */
    });
  });
</script>

<header class="head" use:contextmenu={headerMenu}>
  <button type="button" class="icon back" aria-label="Back to rooms" onclick={() => onback?.()} use:contextmenu={() => [{ id: 'back', label: 'Back to rooms', icon: ChevronLeft, onselect: () => onback?.() }]}>
    <Icon icon={ChevronLeft} size={20} />
  </button>
  <h2 class="heading">
    <button
      type="button"
      class="identity"
      aria-label="Room info for {title}"
      data-tooltip="Room info"
      onclick={() => ui.toggleRoomInfo()}
    >
      <Avatar userId={roomId} displayName={title} src={room.avatar_url} size="sm" shape="rounded" />
      {#if phone}
        <span class="stack">
          <span class="title">{title}</span>
          {#if subtitle}
            <span class="sub">{subtitle}</span>
          {/if}
        </span>
      {:else}
        <span class="title">{title}</span>
        {#if topic}
          <span class="divider" aria-hidden="true"></span>
          <span class="topic">{topic}</span>
        {/if}
      {/if}
    </button>
  </h2>
  <div class="actions">
    <button
      type="button"
      class="icon"
      use:contextmenu={() => [{ id: 'details', label: 'Room details', icon: Info, onselect: () => ui.toggleRoomInfo('info') }]}
      aria-label="Room info"
      aria-pressed={ui.roomInfoOpen && ui.roomInfoTab === 'info'}
      onclick={() => ui.toggleRoomInfo('info')}
    >
      <Icon icon={Info} size={20} />
    </button>
    {#if !phone}
    <button
      type="button"
      class="icon"
      use:contextmenu={() => [{ id: 'members', label: 'Show members', icon: Users, onselect: () => ui.toggleRoomInfo('members') }]}
      aria-label="Member list"
      aria-pressed={ui.roomInfoOpen && ui.roomInfoTab === 'members'}
      onclick={() => ui.toggleRoomInfo('members')}
    >
      <Icon icon={Users} size={20} />
    </button>
    {/if}
    <button
      type="button"
      class="icon"
      use:contextmenu={() => [{ id: 'search', label: 'Search in room', icon: Search, onselect: () => onsearch?.() }]}
      aria-label="Search"
      data-shortcut={searchShortcut}
      onclick={() => onsearch?.()}
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
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    background: var(--alloy-color-surface-canvas);
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
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .identity > :first-child {
    flex: none;
  }
  .identity:hover .title {
    text-decoration: underline;
  }
  .identity:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .title {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .divider {
    flex-shrink: 0;
    width: 1px;
    height: 20px;
    background: var(--alloy-color-border-subtle);
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
    background: var(--alloy-color-surface-hover);
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
  /* Phones: 56px bar, 44px targets with 8px between them, a two-line identity. */
  @media (max-width: 899px) {
    .head {
      height: 56px;
      padding: 0 var(--alloy-spacing-scale-2);
    }
    .actions {
      gap: var(--alloy-spacing-scale-2);
    }
    .icon {
      width: 44px;
      height: 44px;
    }
    .identity {
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
