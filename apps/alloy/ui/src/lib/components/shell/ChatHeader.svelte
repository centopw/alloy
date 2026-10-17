<script lang="ts">
  import type { RoomSummary } from '$lib/generated/protocol';
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronLeft, Search, Users } from '$lib/icons';
  import { membersStore } from '$lib/stores/members.svelte';
  import { roomTitle } from '$lib/stores/rooms.svelte';
  import { ui } from '$lib/stores/ui.svelte';

  interface Props {
    room: RoomSummary;
    onback?: () => void;
    onsearch?: () => void;
  }
  let { room, onback, onsearch }: Props = $props();

  const roomId = $derived(room.id);
  const title = $derived(roomTitle(room));
  const topic = $derived(membersStore.detailsOf(roomId)?.topic?.trim() || null);

  // The topic lives in the room details, which the members panel may not have loaded.
  $effect(() => {
    void membersStore.loadDetails(roomId).catch(() => {
      /* the header simply shows no topic */
    });
  });
</script>

<header class="head">
  <button type="button" class="icon back" aria-label="Back to rooms" title="Back to rooms" onclick={() => onback?.()}>
    <Icon icon={ChevronLeft} size={20} />
  </button>
  <Avatar userId={roomId} displayName={title} src={room.avatar_url} size="sm" shape="rounded" />
  <h2 class="title" title={title}>{title}</h2>
  {#if topic}
    <span class="divider" aria-hidden="true"></span>
    <p class="topic" title={topic}>{topic}</p>
  {/if}
  <div class="actions">
    <button
      type="button"
      class="icon"
      aria-label="Member list"
      aria-pressed={ui.membersOpen}
      title={ui.membersOpen ? 'Hide member list' : 'Show member list'}
      onclick={() => (ui.membersOpen = !ui.membersOpen)}
    >
      <Icon icon={Users} size={20} />
    </button>
    <button type="button" class="icon" aria-label="Search" title="Search (Ctrl K)" onclick={() => onsearch?.()}>
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
    height: 48px;
    padding: 0 var(--alloy-spacing-scale-3);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    background: var(--alloy-color-surface-canvas);
  }
  .title {
    flex: 0 1 auto;
    min-width: 0;
    margin: 0;
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
    flex: 1;
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
    flex-shrink: 0;
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
</style>
