<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronLeft, X } from '$lib/icons';
  import MessageRow from './MessageRow.svelte';
  import Composer from '../composer/Composer.svelte';
  import { threadStore, threadReplies, threadRootItem } from '$lib/stores/thread.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  const rootId = $derived(threadStore.rootEventId ?? '');
  const root = $derived(threadRootItem(threadStore.items, rootId));
  const replies = $derived(threadReplies(threadStore.items, rootId));
</script>

<aside class="thread" aria-label="Thread">
  <header>
    <h2>Thread</h2>
    <button type="button" class="round" aria-label="Close thread" title="Close thread" onclick={onclose}>
      <Icon icon={viewport.isPhone ? ChevronLeft : X} size={20} />
    </button>
  </header>

  <div class="messages">
    {#if root}
      <MessageRow item={root} prev={null} inThread />
    {/if}
    {#each replies as item, i (item.internal_id)}
      <MessageRow {item} prev={i === 0 ? root : replies[i - 1]} inThread />
    {/each}
    {#if threadStore.loading && !root}
      <p class="status">Loading thread…</p>
    {:else if root && replies.length === 0}
      <p class="status">No replies yet.</p>
    {/if}
    {#if threadStore.error}
      <p class="status error" role="alert">{threadStore.error}</p>
    {/if}
  </div>

  {#if threadStore.roomId}
    <Composer roomId={threadStore.roomId} threadRoot={threadStore.rootEventId} />
  {/if}
</aside>

<style>
  .thread {
    display: flex;
    flex-direction: column;
    min-width: 0;
    width: 100%;
    min-height: 0;
    overflow: hidden;
    background: transparent;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    height: var(--alloy-layout-header-height);
    box-sizing: border-box;
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-glass-rim);
  }
  h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }

  .round {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: none;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .round:hover {
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-primary);
  }

  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--alloy-spacing-scale-2) 0;
  }

  .status {
    margin: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .status.error {
    color: var(--alloy-color-status-error);
  }

  /* Phone: a full-screen page with its own back arrow, above the chat. */
  @media (max-width: 899px) {
    .thread {
      position: fixed;
      inset: 0;
      z-index: 50;
      width: auto;
      padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
        var(--alloy-safe-area-left);
      box-sizing: border-box;
      background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
      -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
      backdrop-filter: var(--alloy-glass-filter-strong);
    }
    header {
      justify-content: flex-start;
      height: auto;
      min-height: 56px;
      padding: 0 var(--alloy-spacing-scale-2);
    }
    h2 {
      flex: 1;
      min-width: 0;
      font-size: var(--alloy-typography-size-lg);
      font-weight: var(--alloy-typography-weight-semibold);
    }
    .round {
      width: 44px;
      height: 44px;
    }
    .round:hover {
      background: transparent;
      color: var(--alloy-color-text-secondary);
    }
    .messages {
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
    }
    .status {
      font-size: var(--alloy-typography-size-sm);
    }
  }
</style>
