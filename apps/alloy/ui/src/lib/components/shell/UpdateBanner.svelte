<script lang="ts">
  import { Button } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { X } from '$lib/icons';
  import { AUTO_CHECK_DELAY_MS, updatesStore } from '$lib/stores/updates.svelte';

  const update = $derived(updatesStore.pending);
  const actionLabel = $derived(
    updatesStore.installing ? 'Installing...' : updatesStore.info?.can_install ? 'Update' : 'View release'
  );

  // One silent startup check per app run. The flag flips when the timer fires, so unmounting
  // first clears the timer and a later mount re-arms it.
  $effect(() => {
    if (updatesStore.autoCheckDone) return;
    const timer = setTimeout(() => {
      updatesStore.autoCheckDone = true;
      void updatesStore.check({ silent: true });
    }, AUTO_CHECK_DELAY_MS);
    return () => clearTimeout(timer);
  });
</script>

{#if update}
  <div class="banner" role="status">
    <div class="head">
      <p class="text">Alloy {update.version} is available</p>
      <button type="button" class="dismiss" aria-label="Dismiss update" title="Dismiss update" onclick={() => updatesStore.dismiss()}>
        <Icon icon={X} size={16} />
      </button>
    </div>
    <div class="actions">
      <Button size="sm" disabled={updatesStore.installing} onclick={() => void updatesStore.install()}>{actionLabel}</Button>
    </div>
    {#if updatesStore.error}
      <p class="error" role="alert">{updatesStore.error}</p>
    {/if}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    margin: 0 var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
  }
  .text {
    margin: 0;
    min-width: 0;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    align-items: center;
  }
  .dismiss {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .dismiss:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .error {
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
  .dismiss:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
