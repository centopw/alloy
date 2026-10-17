<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { VList } from 'virtua/svelte';
  import type { VListHandle } from 'virtua/svelte';
  import type { TimelineItemDto } from '$lib/generated';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { session } from '$lib/stores/session.svelte';
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { ArrowDown } from '$lib/icons';
  import MessageRow from './MessageRow.svelte';

  interface Props {
    roomId: string;
    title: string;
    isDirect: boolean;
    encrypted: boolean;
  }
  let { roomId, title, isDirect, encrypted }: Props = $props();

  let vlist: VListHandle | undefined = $state();
  let atBottom = $state(true);
  let shift = $state(false);

  // Virtual items (read marker, timeline start) carry no content worth a row.
  const rows = $derived(timelineStore.items.filter((i) => i.content.kind !== 'Virtual'));
  const myUserId = $derived(session.account?.user_id ?? null);

  // Only `roomId` may retrigger this: the store reads/writes its own state while opening.
  $effect(() => {
    const id = roomId;
    untrack(() => void timelineStore.open(id));
    return () => untrack(() => void timelineStore.close());
  });

  // Keep the viewport stable when older history is prepended; stick to bottom for new messages.
  let lastFirst: string | null = null;
  let lastLen = 0;
  $effect(() => {
    const first = rows[0]?.internal_id ?? null;
    const len = rows.length;
    const prepended = lastFirst !== null && first !== lastFirst && len > lastLen && rows.some((r) => r.internal_id === lastFirst);
    const appended = len > lastLen && !prepended;
    if (prepended) {
      shift = true;
      void tick().then(() => requestAnimationFrame(() => (shift = false)));
    } else if ((appended && atBottom) || (lastLen === 0 && len > 0)) {
      void tick().then(() => vlist?.scrollToIndex(len - 1, { align: 'end' }));
    }
    lastFirst = first;
    lastLen = len;
  });

  function onScroll(offset: number) {
    if (!vlist) return;
    const distance = vlist.getScrollSize() - (offset + vlist.getViewportSize());
    atBottom = distance < 80;
    if (atBottom) void timelineStore.markRead();
    if (offset < 200) void timelineStore.loadOlder();
  }

  function toBottom() {
    vlist?.scrollToIndex(rows.length - 1, { align: 'end', smooth: true });
  }
</script>

<section class="timeline" aria-label="Conversation with {title}">
  <header class="head">
    <Avatar userId={roomId} displayName={title} size="md" />
    <div class="titles">
      <h2>{title}</h2>
      <span class="sub">{encrypted ? 'End-to-end encrypted' : 'Not encrypted'}</span>
    </div>
  </header>

  <div class="viewport">
    {#if timelineStore.loading && rows.length === 0}
      <p class="status" aria-busy="true">Loading messages</p>
    {:else if rows.length === 0}
      <p class="status">No messages yet. Say hello.</p>
    {:else}
      <VList
        bind:this={vlist}
        data={rows}
        getKey={(i: TimelineItemDto) => i.internal_id}
        {shift}
        bufferSize={400}
        onscroll={onScroll}
        style="height: 100%; width: 100%;"
      >
        {#snippet children(item: TimelineItemDto, index: number)}
          <MessageRow
            {item}
            prev={index > 0 ? rows[index - 1] : null}
            next={index < rows.length - 1 ? rows[index + 1] : null}
            {isDirect}
            {myUserId}
          />
        {/snippet}
      </VList>
    {/if}

    {#if !atBottom && rows.length > 0}
      <button class="latest" type="button" onclick={toBottom} aria-label="Jump to latest message">
        <Icon icon={ArrowDown} size={20} />
      </button>
    {/if}

    {#if timelineStore.error}
      <div class="toast" role="alert">{timelineStore.error}</div>
    {/if}
  </div>
</section>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    background: var(--alloy-chat-background, none), var(--alloy-color-surface-canvas);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-4);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    background: var(--alloy-color-surface-panel);
  }
  h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .titles {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .sub {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .viewport {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .status {
    margin: 0;
    padding: var(--alloy-spacing-scale-8);
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
  .latest {
    position: absolute;
    right: var(--alloy-spacing-scale-4);
    bottom: var(--alloy-spacing-scale-4);
    width: 40px;
    height: 40px;
    border-radius: var(--alloy-radii-full);
    border: 1px solid var(--alloy-color-border-prominent);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    display: grid;
    place-items: center;
    cursor: pointer;
    box-shadow: var(--alloy-elevation-md);
  }
  .toast {
    position: absolute;
    left: 50%;
    bottom: var(--alloy-spacing-scale-4);
    transform: translateX(-50%);
    background: var(--alloy-color-status-error);
    color: #fff;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-4);
    border-radius: var(--alloy-radii-md);
  }
</style>
