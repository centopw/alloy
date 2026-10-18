<script lang="ts">
  import { Avatar, Button, UserName } from '../common';
  import { ChevronLeft, ChevronRight, Copy, Icon, Info, MessageSquare, User, X } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { membersStore, memberLabel } from '$lib/stores/members.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import { roomsStore } from '$lib/stores/rooms.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import { userProfile } from '$lib/stores/userProfile.svelte';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import type { RoomMemberDto } from '$lib/generated';

  interface Props {
    roomId: string;
    /** Shown as a close button when the panel overlays the chat instead of having its own column. */
    onclose?: () => void;
  }

  let { roomId, onclose }: Props = $props();

  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  // Bumping this re-runs the load effect (the retry button).
  let attempt = $state(0);
  // Pointer position of the last right-click on a member, used to place the profile card from the menu.
  let menuAnchor = $state({ x: 0, y: 0 });

  const details = $derived(membersStore.detailsOf(roomId));
  const members = $derived(membersStore.membersOf(roomId));

  function byName(a: RoomMemberDto, b: RoomMemberDto): number {
    return memberLabel(a).localeCompare(memberLabel(b));
  }

  const joined = $derived(members.filter((m) => m.membership === 'Joined'));
  const admins = $derived(joined.filter((m) => m.power_level >= 100).sort(byName));
  const moderators = $derived(joined.filter((m) => m.power_level >= 50 && m.power_level < 100).sort(byName));
  const regular = $derived(joined.filter((m) => m.power_level < 50).sort(byName));
  const invited = $derived(members.filter((m) => m.membership === 'Invited').sort(byName));

  const groups = $derived(
    [
      { title: 'Admin', members: admins },
      { title: 'Moderator', members: moderators },
      { title: 'Members', members: regular },
      { title: 'Invited', members: invited }
    ].filter((g) => g.members.length > 0)
  );

  // Reload whenever the active room changes (or on retry). `force` refetches the member list
  // so the panel never shows a stale roster; the cached values stay visible until replaced.
  $effect(() => {
    const id = roomId;
    void attempt;
    let current = true;
    loading = true;
    loadError = null;
    void Promise.allSettled([membersStore.loadDetails(id), membersStore.loadMembers(id, true)]).then((results) => {
      if (!current) return;
      const failed = results.find((r): r is PromiseRejectedResult => r.status === 'rejected');
      loadError = failed ? errorMessage(failed.reason) : null;
      loading = false;
    });
    return () => {
      current = false;
    };
  });

  /** Opens the profile card at the pointer, or at the row centre when the row was activated from the keyboard. */
  function openProfile(e: MouseEvent, userId: string) {
    let at = { x: e.clientX, y: e.clientY };
    if (e.detail === 0) {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      at = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }
    actionError = null;
    profileCard.open(userId, roomId, at);
  }

  async function message(userId: string) {
    actionError = null;
    try {
      const dmRoomId = await userProfile.startDirectMessage(userId);
      roomsStore.select(dmRoomId);
    } catch (e) {
      actionError = errorMessage(e);
    }
  }

  function memberMenu(m: RoomMemberDto): MenuItem[] {
    return [
      {
        id: 'view-profile',
        label: 'View profile',
        icon: User,
        onselect: () => profileCard.open(m.user_id, roomId, menuAnchor)
      },
      { id: 'message', label: 'Message', icon: MessageSquare, onselect: () => void message(m.user_id) },
      { separator: true },
      { id: 'copy-user-id', label: 'Copy user ID', icon: Copy, onselect: () => void navigator.clipboard.writeText(m.user_id) }
    ];
  }
</script>

{#snippet memberRow(m: RoomMemberDto)}
  <li>
    <button
      type="button"
      class="row"
      class:invited={m.membership === 'Invited'}
      data-user-id={m.user_id}
      onclick={(e) => openProfile(e, m.user_id)}
      oncontextmenu={(e) => (menuAnchor = { x: e.clientX, y: e.clientY })}
      use:contextmenu={() => memberMenu(m)}
    >
      <Avatar
        userId={m.user_id}
        displayName={memberLabel(m)}
        src={m.avatar_url}
        size="sm"
        shape="circle"
      />
      <span class="who">
        <UserName userId={m.user_id} displayName={memberLabel(m)} />
      </span>
    </button>
  </li>
{/snippet}

<div class="members-panel" role="complementary" aria-label="Members">
  <header class="bar">
    <h2>Members</h2>
    {#if onclose}
      <button type="button" class="close" aria-label="Close members" onclick={() => onclose?.()}>
        <Icon icon={viewport.isPhone ? ChevronLeft : X} size={20} />
      </button>
    {/if}
  </header>

  <button type="button" class="room-info" onclick={() => ui.openRoomInfo('info')}>
    <Icon icon={Info} size={16} />
    <span>Room info</span>
    <Icon icon={ChevronRight} size={16} />
  </button>

  {#if loadError}
    <div class="notice" role="alert">
      <p>Could not load room details: {loadError}</p>
      <Button variant="ghost" size="sm" onclick={() => (attempt += 1)}>Try again</Button>
    </div>
  {/if}
  {#if actionError}
    <div class="notice" role="alert">
      <p>{actionError}</p>
    </div>
  {/if}

  {#if details}
    <section class="info">
      {#if details.topic?.trim()}
        <p class="topic">{details.topic.trim()}</p>
      {/if}
      <p class="fact">{details.encrypted ? 'End-to-end encrypted' : 'Not encrypted'}</p>
      <p class="fact">{details.joined_members} {details.joined_members === 1 ? 'member' : 'members'}</p>
    </section>
  {:else if loading && !loadError}
    <p class="notice" role="status">Loading room details</p>
  {/if}

  <div class="list">
    {#if loading && members.length === 0}
      <p class="notice" role="status">Loading members</p>
    {:else}
      {#each groups as group (group.title)}
        <section class="group">
          <h3>{group.title} ({group.members.length})</h3>
          <ul class="rows">
            {#each group.members as m (m.user_id)}
              {@render memberRow(m)}
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>
</div>

<style>
  .members-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    min-width: 0;
    height: 100%;
    min-height: 0;
    background: var(--alloy-color-surface-panel);
    color: var(--alloy-color-text-primary);
  }
  .bar {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    box-sizing: border-box;
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  .bar h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-base);
  }
  .close {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    margin-inline-end: calc(var(--alloy-layout-gutter) * -1);
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .close:hover {
    background: var(--alloy-color-surface-hover);
  }
  .close:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .room-info {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    margin: var(--alloy-layout-row-inset);
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    text-align: start;
    cursor: pointer;
  }
  .room-info span {
    flex: 1;
  }
  .room-info:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .room-info:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .info {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-layout-section-gap) var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  .topic {
    margin: 0 0 var(--alloy-spacing-scale-1);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .fact {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 0 var(--alloy-layout-section-gap);
  }
  h3 {
    margin: var(--alloy-layout-section-gap) 0 var(--alloy-spacing-scale-2);
    padding: 0 calc(var(--alloy-layout-row-inset) + var(--alloy-layout-row-pad-x));
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0 var(--alloy-layout-row-inset);
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    margin: 0;
    padding: var(--alloy-spacing-scale-1) var(--alloy-layout-row-pad-x);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .row:hover {
    background: var(--alloy-color-surface-hover);
  }
  .row:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
  .row.invited {
    opacity: 0.6;
  }
  .who {
    display: flex;
    flex: 1;
    min-width: 0;
    font-size: var(--alloy-typography-size-sm);
  }
  .notice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-2);
    margin: var(--alloy-layout-section-gap) var(--alloy-layout-gutter);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .notice[role='alert'] {
    color: var(--alloy-color-status-error);
  }
  .notice p {
    margin: 0;
  }

  /* Phone: a full-screen page with a back arrow, and touch-sized rows. */
  @media (max-width: 899px) {
    .members-panel {
      position: fixed;
      inset: 0;
      z-index: 50;
      box-sizing: border-box;
      padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
        var(--alloy-safe-area-left);
    }
    .bar {
      height: var(--alloy-layout-header-height);
      padding: 0 var(--alloy-spacing-scale-2);
      justify-content: flex-start;
    }
    .bar h2 {
      flex: 1;
      min-width: 0;
      font-size: var(--alloy-typography-size-lg);
      font-weight: var(--alloy-typography-weight-semibold);
    }
    .close {
      order: -1;
      width: 44px;
      height: 44px;
      margin-inline: 0;
    }
    .close:hover {
      background: transparent;
    }
    .room-info {
      min-height: 56px;
      font-size: var(--alloy-typography-size-lg);
    }
    .room-info:hover {
      background: transparent;
      color: var(--alloy-color-text-secondary);
    }
    .room-info:active {
      background: var(--alloy-color-surface-hover);
    }
    .list {
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
    }
    .row {
      min-height: 56px;
    }
    .row:hover {
      background: transparent;
    }
    .row:active {
      background: var(--alloy-color-surface-hover);
    }
    .who {
      font-size: var(--alloy-typography-size-lg);
    }
  }
</style>
