<script lang="ts">
  import { openExternal } from '$lib/platform/links';
  import { Button } from '$lib/components/common';
  import { updatesStore } from '$lib/stores/updates.svelte';
  import pkg from '../../../../../package.json';

  const REPOSITORY_URL = 'https://github.com/centopw/alloy';

  function openRepository(e: MouseEvent) {
    e.preventDefault();
    void openExternal(REPOSITORY_URL);
  }

  const info = $derived(updatesStore.info);
  const available = $derived(info?.available ? info : null);
  const actionLabel = $derived(
    updatesStore.installing ? 'Installing...' : info?.can_install ? 'Update' : 'View release'
  );
</script>

<section class="section">
  <div class="card">
    <div class="row">
      <span class="label">Application</span>
      <span class="value">Alloy</span>
    </div>
    <div class="row">
      <span class="label">Version</span>
      <span class="value">{pkg.version}</span>
    </div>
    <div class="row">
      <span class="label">Licence</span>
      <span class="value">Apache-2.0</span>
    </div>
    <div class="row">
      <span class="label">Source</span>
      <a class="link" href={REPOSITORY_URL} onclick={openRepository}>{REPOSITORY_URL}</a>
    </div>
    <div class="row updates">
      <span class="label">Updates</span>
      <div class="update-status">
        {#if updatesStore.checking}
          <span class="status" role="status">Checking...</span>
        {:else if available}
          <span class="status" role="status">Version {available.version} is available</span>
        {:else if info}
          <span class="status" role="status">Alloy is up to date</span>
        {/if}
        {#if updatesStore.error}
          <p class="update-error" role="alert">{updatesStore.error}</p>
        {/if}
        <div class="update-actions">
          {#if available && !updatesStore.checking}
            <Button size="sm" disabled={updatesStore.installing} onclick={() => void updatesStore.install()}>{actionLabel}</Button>
          {/if}
          <Button variant="ghost" size="sm" disabled={updatesStore.checking || updatesStore.installing} onclick={() => void updatesStore.check()}>Check for updates</Button>
        </div>
      </div>
    </div>
  </div>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .card {
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
    font-size: var(--alloy-typography-size-base);
    color: var(--alloy-color-text-primary);
  }
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .label {
    color: var(--alloy-color-text-secondary);
  }
  .value {
    overflow-wrap: anywhere;
    text-align: right;
  }
  .link {
    color: var(--alloy-color-text-link);
    overflow-wrap: anywhere;
    text-align: right;
    border-radius: var(--alloy-radii-sm);
  }
  .link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .updates {
    align-items: flex-start;
  }
  .update-status {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    font-size: var(--alloy-typography-size-sm);
  }
  .status {
    color: var(--alloy-color-text-secondary);
  }
  .update-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  .update-error {
    margin: 0;
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
    text-align: right;
  }
</style>
