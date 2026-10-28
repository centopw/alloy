<script lang="ts">
  // Bits UI supplies the progressbar role and aria-valuenow; ADR 0012 replaced the hand-rolled
  // aria-hidden track. Restyle through `class`; the vendored primitive stays unmodified.
  import { Progress } from '$lib/components/ui/progress/index.js';
  import Icon from '$lib/icons/Icon.svelte';
  import { X } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { updatesStore } from '$lib/stores/updates.svelte';

  const visible = $derived(updatesStore.pending !== null || updatesStore.busy);
  const percent = $derived(
    updatesStore.progress === null ? null : Math.round(updatesStore.progress * 100),
  );

  /** Two quiet lines: what is happening, and what the user may do about it. */
  const copy = $derived.by(() => {
    const version = updatesStore.info?.version;
    switch (updatesStore.phase) {
      case 'error':
        return { title: 'Update failed', detail: 'You can keep using Alloy', action: 'Retry' };
      case 'installing':
        return { title: 'Installing update', detail: 'Almost done', action: null };
      case 'downloading':
        return {
          title: 'Downloading update',
          detail: percent === null ? 'Working in the background' : `${percent}%`,
          action: null,
        };
      case 'ready':
        return { title: 'Update ready', detail: 'Restart to apply', action: 'Restart' };
      default:
        return {
          title: 'Update available',
          detail: version ? `Version ${version}` : "See what's new",
          action: updatesStore.info?.can_install === false ? 'Open' : 'Download',
        };
    }
  });

  function menuItems(): MenuItem[] {
    const items: MenuItem[] = [
      { id: 'whats-new', label: "What's new", onselect: () => updatesStore.openChangelog() },
      {
        id: 'primary',
        label: updatesStore.primaryLabel,
        disabled: updatesStore.busy,
        onselect: () => void updatesStore.primary(),
      },
      { id: 'remind-later', label: 'Remind me later', onselect: () => updatesStore.remindLater() },
    ];
    // Once the download has started, skipping would leave a downloaded update unused.
    if (!updatesStore.info?.downloaded && !updatesStore.busy) {
      items.push({ id: 'skip-version', label: 'Skip this version', onselect: () => updatesStore.skipVersion() });
    }
    items.push({ id: 'check', label: 'Check for updates', onselect: () => void updatesStore.check() });
    return items;
  }
</script>

{#if visible}
  <div class="notice" class:failed={updatesStore.phase === 'error'} use:contextmenu={menuItems}>
    <button type="button" class="body" onclick={() => updatesStore.openChangelog()}>
      <span class="dot" aria-hidden="true"></span>
      <span class="text">
        <span class="title">{copy.title}</span>
        <span class="detail">{copy.detail}</span>
      </span>
    </button>
    {#if copy.action}
      <button type="button" class="action" onclick={() => void updatesStore.primary()}>{copy.action}</button>
    {/if}
    {#if !updatesStore.busy}
      <button
        type="button"
        class="dismiss"
        aria-label="Remind me later"
        data-tooltip="Remind me later"
        onclick={() => updatesStore.remindLater()}
      >
        <Icon icon={X} size={16} />
      </button>
    {/if}
    {#if updatesStore.phase === 'downloading'}
      <!--
        Known size: a determinate bar. Unknown size (null) keeps correct indeterminate ARIA from
        Bits UI but renders an empty track, so the track itself pulses to read as "still working"
        rather than stalled — the same intent as the old 30% placeholder.
      -->
      <Progress
        value={percent}
        aria-label={percent === null ? 'Working on update' : 'Downloading update'}
        class="absolute inset-x-0 bottom-0 h-[2px] w-full overflow-hidden rounded-none
          bg-[var(--alloy-color-border-subtle)]
          [&_[data-slot=progress-indicator]]:bg-[var(--alloy-color-accent-primary)]
          [&_[data-slot=progress-indicator]]:duration-200
          {percent === null ? 'animate-pulse' : ''}"
      />
    {/if}
  </div>
{/if}

<style>
  .notice {
    position: relative;
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    margin: var(--alloy-layout-row-inset) var(--alloy-layout-row-inset) 0;
    padding: 0 var(--alloy-spacing-scale-1) 0 0;
    overflow: hidden;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-accent-subtle);
  }

  .body {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    min-height: 44px;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-1) var(--alloy-layout-row-pad-x);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radius-full);
    background: var(--alloy-color-accent-primary);
  }

  .failed .dot {
    background: var(--alloy-color-text-muted);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .title,
  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .detail {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-xs);
  }

  .action,
  .dismiss {
    flex: none;
    min-height: 44px;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    font: inherit;
    cursor: pointer;
  }

  .action {
    padding: 0 var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-link);
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 44px;
    color: var(--alloy-color-text-muted);
  }

  .action:hover,
  .dismiss:hover {
    background: var(--alloy-color-surface-hover);
  }

  .dismiss:hover {
    color: var(--alloy-color-text-primary);
  }

  .body:focus-visible,
  .action:focus-visible,
  .dismiss:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
</style>
