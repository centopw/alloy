<script lang="ts">
  import { Avatar, Button, UserName } from '../common';
  import { membersStore, memberLabel } from '$lib/stores/members.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import type { RoomMemberDto } from '$lib/generated/index.js';

  interface Props {
    roomId: string;
  }

  let { roomId }: Props = $props();

  let loading = $state(true);
  let loadError = $state<string | null>(null);
  // Bumping this re-runs the load effect (the retry button).
  let attempt = $state(0);

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
</script>

{#snippet memberRow(m: RoomMemberDto)}
  <li class="row" class:invited={m.membership === 'Invited'}>
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
  </li>
{/snippet}

<div class="members-panel" role="complementary" aria-label="Members">
  {#if loadError}
    <div class="notice" role="alert">
      <p>Could not load room details: {loadError}</p>
      <Button variant="ghost" size="sm" onclick={() => (attempt += 1)}>Try again</Button>
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
  .info {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    flex: none;
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
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
    padding: 0 var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-4);
  }
  h3 {
    margin: var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    min-width: 0;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-md);
  }
  .row:hover {
    background: var(--alloy-color-surface-hover);
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
    margin: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .notice[role='alert'] {
    color: var(--alloy-color-status-error);
  }
  .notice p {
    margin: 0;
  }
</style>
