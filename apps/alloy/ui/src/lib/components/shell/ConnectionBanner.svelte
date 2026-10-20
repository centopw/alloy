<script lang="ts">
  import Button from '../common/Button.svelte';
  import { connectivity } from '$lib/stores/connectivity.svelte';

  const text = $derived(
    connectivity.browserOnline ? "Can't reach your server. Trying again\u2026" : "You're offline. Waiting for a connection\u2026"
  );
</script>

{#if connectivity.offline}
  <div class="banner" role="status" aria-live="polite">
    <span class="text">{text}</span>
    <Button variant="ghost" size="sm" onclick={() => void connectivity.retryNow()}>Retry now</Button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--alloy-spacing-scale-3);
    flex-shrink: 0;
    /* The shell drops its own top inset while this bar is shown, so the bar keeps clear of the status bar. */
    padding: calc(var(--alloy-safe-area-top) + var(--alloy-spacing-scale-1)) var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-1);
    background: var(--alloy-color-surface-elevated);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm, 12px);
    text-align: center;
  }
  .text {
    min-width: 0;
  }
</style>
