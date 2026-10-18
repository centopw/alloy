<script lang="ts">
  import type { TimelineItemContent, TimelineItemDto } from '$lib/generated';
  import { Button } from '../common';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { pollPercent } from './pollResults';

  type PollData = Extract<TimelineItemContent, { kind: 'Poll' }>['data'];

  interface Props {
    item: TimelineItemDto;
    poll: PollData;
  }
  let { item, poll }: Props = $props();

  const multiple = $derived(poll.max_selections > 1);
  const open = $derived(!poll.has_ended);
  // The engine already hides counts while an undisclosed poll is open; this only decides presentation.
  const revealed = $derived(poll.disclosed || poll.has_ended);
  const atLimit = $derived(multiple && poll.my_votes.length >= poll.max_selections);

  function choose(answerId: string) {
    if (!open) return;
    const mine = poll.my_votes;
    const selected = mine.includes(answerId);
    let next: string[];
    if (multiple) {
      next = selected ? mine.filter((id) => id !== answerId) : [...mine, answerId];
    } else {
      next = selected ? [] : [answerId];
    }
    void timelineStore.respondToPoll(item, next);
  }
</script>

<div class="poll">
  <p class="question">{poll.question}</p>
  <div class="answers" role={multiple ? 'group' : 'radiogroup'} aria-label={poll.question}>
    {#each poll.answers as answer (answer.id)}
      {@const selected = poll.my_votes.includes(answer.id)}
      {@const pct = pollPercent(answer.votes, poll.total_votes)}
      <button
        type="button"
        class="answer"
        class:selected
        role={multiple ? 'checkbox' : 'radio'}
        aria-checked={selected}
        disabled={!open || (!selected && atLimit)}
        onclick={() => choose(answer.id)}
      >
        {#if revealed}<span class="bar" style="width: {pct}%"></span>{/if}
        <span class="label">{answer.text}</span>
        {#if revealed}<span class="count">{pct}%</span>{/if}
      </button>
    {/each}
  </div>
  <div class="meta">
    {#if revealed}
      <span>{poll.total_votes} {poll.total_votes === 1 ? 'vote' : 'votes'}</span>
    {:else}
      <span>Results appear when the poll ends</span>
    {/if}
    {#if multiple && open}<span>Choose up to {poll.max_selections}</span>{/if}
    {#if poll.has_ended}<span>Poll ended</span>{/if}
    {#if item.is_own && open}
      <Button variant="ghost" size="sm" onclick={() => timelineStore.endPoll(item)}>End poll</Button>
    {/if}
  </div>
</div>

<style>
  .poll {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    max-width: 420px;
  }
  .question {
    margin: 0;
    font-weight: 600;
    color: var(--alloy-color-text-primary);
  }
  .answers {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .answer {
    position: relative;
    overflow: hidden;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    text-align: left;
    color: var(--alloy-color-text-primary);
    background: var(--alloy-color-surface-subtle);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    cursor: pointer;
  }
  .answer.selected {
    border-color: var(--alloy-color-accent-primary);
  }
  .answer:disabled {
    cursor: default;
  }
  .answer:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .bar {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--alloy-color-accent-subtle);
    pointer-events: none;
  }
  .label,
  .count {
    position: relative;
    z-index: 1;
  }
  .count {
    color: var(--alloy-color-text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  @media (max-width: 899px) {
    .poll {
      max-width: 100%;
    }
    .answer {
      min-height: 44px;
      font-size: var(--alloy-typography-size-base);
    }
    .meta :global(button) {
      min-height: 44px;
    }
  }
</style>
