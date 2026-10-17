<script lang="ts">
  import { Toggle } from '$lib/components/common';
  import { ui } from '$lib/stores/ui.svelte';
  import pkg from '../../../../../package.json';

  // Mirrors the selection rule in ipc/index.ts (the active transport is not exported from there).
  function engineLabel(): string {
    if (typeof window === 'undefined') return 'Unknown';
    if ('__TAURI_INTERNALS__' in window) return 'Tauri IPC';
    if (new URLSearchParams(window.location.search).get('engine')) return 'Bridge WebSocket';
    return 'In-memory mock';
  }
  const engine = engineLabel();
</script>

<section class="section" aria-labelledby="advanced-title">
  <h2 id="advanced-title" class="title">Advanced</h2>

  {#if import.meta.env.DEV}
    <div class="card">
      <div class="row">
        <span>Developer HUD</span>
        <Toggle label="Developer HUD" bind:checked={ui.hudVisible} />
      </div>
    </div>
  {/if}

  <h3 class="subtitle">Diagnostics</h3>
  <div class="card">
    <div class="row">
      <span class="label">App version</span>
      <span class="value">{pkg.version}</span>
    </div>
    <div class="row">
      <span class="label">Engine</span>
      <span class="value">{engine}</span>
    </div>
  </div>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }
  .title {
    margin: 0;
    font-size: var(--alloy-typography-size-xl);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .subtitle {
    margin: var(--alloy-spacing-scale-4) 0 0;
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
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
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
    font-family: var(--alloy-typography-font-family-mono);
    overflow-wrap: anywhere;
    text-align: right;
  }
</style>
