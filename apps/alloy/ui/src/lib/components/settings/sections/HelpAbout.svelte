<script lang="ts">
  import { Button } from '$lib/components/common';
  import { SettingToggle } from '$lib/components/settings/controls';
  import { openExternal } from '$lib/platform/links';
  import { ui } from '$lib/stores/ui.svelte';
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

  /** Names the engine transport this UI is talking through. */
  function engineLabel(): string {
    if (typeof window === 'undefined') return 'Unknown';
    if ('__TAURI_INTERNALS__' in window) return 'Tauri IPC';
    if (new URLSearchParams(window.location.search).has('engine')) return 'Bridge WebSocket';
    return 'In-memory mock';
  }
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
      <a class="value link" href={REPOSITORY_URL} onclick={openRepository}>{REPOSITORY_URL}</a>
    </div>
    <div class="row updates">
      <span class="label">Updates</span>
      <div class="update-status">
        <span class="status" role="status">
          {#if updatesStore.checking}
            Checking...
          {:else if available}
            Version {available.version} is available
          {:else}
            Alloy is up to date
          {/if}
        </span>
        {#if updatesStore.error}
          <p class="update-error" role="alert">{updatesStore.error}</p>
        {/if}
      </div>
      <div class="update-actions">
        {#if available && !updatesStore.checking}
          <Button
            size="sm"
            disabled={updatesStore.installing}
            onclick={() => void updatesStore.install()}>{actionLabel}</Button
          >
        {/if}
        <Button
          variant="ghost"
          size="sm"
          disabled={updatesStore.checking || updatesStore.installing}
          onclick={() => void updatesStore.check()}>Check for updates</Button
        >
      </div>
    </div>
  </div>

  <div class="block">
    <h3 class="subtitle">Diagnostics</h3>
    <div class="card">
      <div class="row">
        <span class="label">App version</span>
        <span class="value mono">{pkg.version}</span>
      </div>
      <div class="row">
        <span class="label">Engine</span>
        <span class="value mono">{engineLabel()}</span>
      </div>
    </div>
  </div>

  {#if import.meta.env.DEV}
    <div class="block">
      <h3 class="subtitle">Developer</h3>
      <SettingToggle
        label="Developer HUD"
        description="Shows engine and render diagnostics over the app."
        checked={ui.hudVisible}
        onchange={(v) => (ui.hudVisible = v)}
      />
    </div>
  {/if}
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .subtitle {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
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
    min-width: 0;
    text-align: right;
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--alloy-typography-font-family-mono);
  }
  .link {
    color: var(--alloy-color-link);
    text-decoration: none;
  }
  .link:hover {
    text-decoration: underline;
  }
  .link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .updates {
    flex-wrap: wrap;
    align-items: flex-start;
  }
  .update-status {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .status {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .update-actions {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
  }
  .update-error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-sm);
    overflow-wrap: anywhere;
  }
</style>
