<script lang="ts">
  import type { TimelineItemDto } from '$lib/generated';
  import { Avatar, Modal, UserName } from '../common';
  import { reactionTabs, reactorRows } from '$lib/content/reactors';
  import { membersStore } from '$lib/stores/members.svelte';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { session } from '$lib/stores/session.svelte';

  interface Props {
    item: TimelineItemDto;
    open: boolean;
    /** Tab to show first; `null` is "All". */
    initialKey?: string | null;
    onclose: () => void;
  }
  let { item, open, initialKey = null, onclose }: Props = $props();

  let selected = $state<string | null>(null);
  const roomId = $derived(timelineStore.roomId);
  const reactions = $derived(item.reactions ?? []);
  const tabs = $derived(reactionTabs(reactions));
  const rows = $derived(reactorRows(reactions, selected));

  $effect(() => {
    if (!open) return;
    selected = initialKey;
    if (roomId) void membersStore.loadMembers(roomId);
  });

  // A tab whose reactions were all removed falls back to All.
  $effect(() => {
    if (selected !== null && !reactions.some((r) => r.key === selected)) selected = null;
  });

  // Nothing left to show once the last reaction is gone.
  $effect(() => {
    if (open && reactions.length === 0) onclose();
  });

  function member(userId: string) {
    return membersStore.membersOf(roomId).find((m) => m.user_id === userId);
  }
</script>

<Modal {open} title="Reactions" {onclose} width="420px">
  <div class="tabs" role="tablist" aria-label="Reactions by emoji">
    {#each tabs as tab (tab.key ?? 'all')}
      <button
        type="button"
        role="tab"
        class="tab"
        aria-selected={selected === tab.key}
        onclick={() => (selected = tab.key)}
      >
        <span>{tab.key ?? 'All'}</span><span class="count">{tab.count}</span>
      </button>
    {/each}
  </div>

  <ul class="list">
    {#each rows as row (row.userId + row.key)}
      {@const m = member(row.userId)}
      <li class="person">
        <Avatar userId={row.userId} displayName={m?.display_name ?? null} src={m?.avatar_url ?? null} size={32} />
        <span class="name"><UserName userId={row.userId} displayName={m?.display_name ?? null} {roomId} /></span>
        <span class="emoji">{row.key}</span>
        {#if row.userId === session.account?.user_id}
          <button type="button" class="remove" onclick={() => void timelineStore.toggleReaction(item, row.key)}>Remove</button>
        {/if}
      </li>
    {/each}
  </ul>
</Modal>

<style>
  .tabs {
    display: flex;
    gap: var(--alloy-spacing-scale-1);
    padding-bottom: var(--alloy-spacing-scale-2);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-height: 32px;
    padding: 0 var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    cursor: pointer;
  }
  .tab[aria-selected='true'] {
    border-color: var(--alloy-color-accent-primary);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-text-primary);
  }
  .count {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .person {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    min-height: 44px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .emoji {
    font-size: 18px;
  }
  .remove {
    min-height: 32px;
    padding: 0 var(--alloy-spacing-scale-3);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
    font: inherit;
    cursor: pointer;
  }
  @media (hover: none) {
    .tab,
    .remove {
      min-height: 44px;
    }
  }
</style>
