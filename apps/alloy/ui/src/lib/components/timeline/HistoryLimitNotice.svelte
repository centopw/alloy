<script lang="ts">
  import type { HistoryLimit } from '$lib/generated';
  import { historyLimitNotice } from './historyLimit';

  interface Props {
    /** The reason paging stopped short; `null` while nothing was reported. */
    limit: HistoryLimit | null;
  }

  let { limit }: Props = $props();

  const text = $derived(historyLimitNotice(limit));
</script>

{#if text}
  <p class="history-limit" role="note">{text}</p>
{/if}

<style>
  .history-limit {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    background: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-font-size-preview);
    text-align: center;
  }
</style>
