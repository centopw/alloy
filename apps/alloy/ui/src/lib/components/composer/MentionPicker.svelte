<script lang="ts">
  import type { RoomMemberDto } from '$lib/generated/index.js';
  import { Avatar, UserName } from '../common';
  import { memberLabel } from '$lib/stores/members.svelte';
  import { mentionOptionId } from '$lib/stores/mentions';

  interface Props {
    /** DOM id of the listbox; option ids derive from it. */
    id: string;
    candidates: RoomMemberDto[];
    /** Index of the option highlighted by the keyboard. */
    highlighted: number;
    onpick: (member: RoomMemberDto) => void;
    onhover: (index: number) => void;
  }

  let { id, candidates, highlighted, onpick, onhover }: Props = $props();
</script>

<!-- Focus stays in the textarea: keyboard selection is handled there, so mousedown must not blur it. -->
<div
  {id}
  class="picker"
  role="listbox"
  aria-label="Members"
  tabindex="-1"
  onmousedown={(e) => e.preventDefault()}
>
  {#each candidates as member, i (member.user_id)}
    <div
      id={mentionOptionId(id, i)}
      class="option"
      class:active={i === highlighted}
      role="option"
      tabindex="-1"
      aria-selected={i === highlighted}
      data-user-id={member.user_id}
      onmouseenter={() => onhover(i)}
      onclick={() => onpick(member)}
      onkeydown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onpick(member);
        }
      }}
    >
      <Avatar userId={member.user_id} displayName={member.display_name} size="xs" />
      <span class="name">
        <UserName userId={member.user_id} displayName={memberLabel(member)} />
      </span>
      <span class="meta id">{member.user_id}</span>
      {#if member.membership === 'Invited'}
        <span class="meta">Invited</span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .picker {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + var(--alloy-spacing-scale-2));
    z-index: 5;
    max-height: 240px;
    overflow-y: auto;
    padding: var(--alloy-spacing-scale-1);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-elevation-md);
  }
  .option {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-md);
    cursor: pointer;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }
  .option.active {
    background: var(--alloy-color-surface-hover);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-xs);
  }
  .id {
    margin-left: auto;
  }
</style>
