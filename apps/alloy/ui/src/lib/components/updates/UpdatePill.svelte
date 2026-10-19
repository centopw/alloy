<script lang="ts">
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { updatesStore } from '$lib/stores/updates.svelte';

  const visible = $derived(updatesStore.pending !== null || updatesStore.busy);
  const percent = $derived(
    updatesStore.progress === null ? null : Math.round(updatesStore.progress * 100),
  );

  const label = $derived.by(() => {
    switch (updatesStore.phase) {
      case 'error':
        return 'Update failed, retry';
      case 'installing':
        return 'Installing update';
      case 'downloading':
        return percent === null ? 'Downloading update' : `Downloading ${percent}%`;
      case 'ready':
        return 'Restart to apply update';
      default:
        return 'Update';
    }
  });

  function menuItems(): MenuItem[] {
    return [
      { id: 'whats-new', label: "What's new", onselect: () => updatesStore.openChangelog() },
      {
        id: 'primary',
        label: updatesStore.primaryLabel,
        disabled: updatesStore.busy,
        onselect: () => void updatesStore.primary(),
      },
      { id: 'remind-later', label: 'Remind me later', onselect: () => updatesStore.remindLater() },
      { id: 'skip-version', label: 'Skip this version', onselect: () => updatesStore.skipVersion() },
      { id: 'check', label: 'Check for updates', onselect: () => void updatesStore.check() },
    ];
  }

  function activate() {
    if (!updatesStore.busy) void updatesStore.primary();
  }
</script>

{#if visible}
  <button
    type="button"
    class="pill"
    class:ready={updatesStore.phase === 'ready'}
    class:failed={updatesStore.phase === 'error'}
    aria-disabled={updatesStore.busy}
    onclick={activate}
    use:contextmenu={menuItems}
  >
    <span class="label">{label}</span>
    {#if updatesStore.phase === 'downloading'}
      <span class="track" aria-hidden="true">
        <span class="fill" style:width={percent === null ? '30%' : `${percent}%`}></span>
      </span>
    {/if}
  </button>
{/if}

<style>
  .pill {
    position: relative;
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 44px;
    max-width: 100%;
    padding: 0 var(--alloy-spacing-scale-3);
    overflow: hidden;
    border: 1px solid var(--alloy-color-accent-primary);
    border-radius: var(--alloy-radius-full);
    background: transparent;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    white-space: nowrap;
    cursor: pointer;
  }

  .pill[aria-disabled='true'] {
    cursor: progress;
  }

  .pill.ready {
    background: var(--alloy-color-accent-fill);
    border-color: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
  }

  .pill.failed {
    border-color: var(--alloy-color-border-prominent);
    color: var(--alloy-color-text-primary);
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    height: 2px;
    background: var(--alloy-color-border-subtle);
  }

  .fill {
    display: block;
    height: 100%;
    background: var(--alloy-color-accent-primary);
    transition: width 200ms ease;
  }

  .pill:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
