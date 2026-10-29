<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/common';
  import { Icon, Bell, Mic, MapPin, Smartphone, BatteryCharging, Check } from '$lib/icons';
  import { actionFor, stateLabel } from '$lib/content/permissionsModel';
  import type { PermissionId } from '$lib/platform/permissions';
  import { permissionsStore, type PermissionRow } from '$lib/stores/permissions.svelte';

  const ICONS = {
    notifications: Bell,
    microphone: Mic,
    location: MapPin,
    'push-app': Smartphone,
    battery: BatteryCharging,
  } as const;

  let busy = $state<PermissionId | null>(null);

  // States are read when the panel shows and again on focus, visibility and the Android answer event.
  onMount(() => permissionsStore.watch());

  async function allow(row: PermissionRow) {
    busy = row.id;
    try {
      await permissionsStore.request(row.id);
    } finally {
      busy = null;
    }
  }

  async function openSettings(row: PermissionRow) {
    busy = row.id;
    try {
      permissionsStore.openSettings(row.id);
    } finally {
      busy = null;
    }
  }
  const summary = $derived(permissionsStore.summary);
</script>

<div class="panel">
  <p class="summary" role="status" aria-live="polite" data-testid="permissions-summary">
    {permissionsStore.loaded ? summary.text : 'Checking what is allowed…'}
  </p>
  <ul class="list">
    {#each permissionsStore.rows as row (row.id)}
      {@const action = actionFor(row.state, row.canOpenSettings)}
      <li class="row" data-permission={row.id} data-state={row.state}>
        <span class="icon" aria-hidden="true"><Icon icon={ICONS[row.id]} size={20} /></span>
        <div class="text">
          <div class="head">
            <span class="label">{row.label}</span>
            <span class="tag" class:required={row.required}>{row.required ? 'Required' : 'Optional'}</span>
          </div>
          <span class="why">{row.why}</span>
        </div>
        <div class="end">
          <span class="pill" class:ok={row.state === 'granted'} class:bad={row.state === 'denied'}>
            {#if row.state === 'granted'}<Icon icon={Check} size={16} />{/if}
            {stateLabel(row.state)}
          </span>
          {#if action === 'allow'}
            <Button variant="primary" size="sm" disabled={busy === row.id} onclick={() => allow(row)}>
              {row.id === 'push-app' ? 'Choose app' : row.id === 'battery' ? 'Open settings' : 'Allow'}
            </Button>
          {:else if action === 'open-settings'}
            <Button variant="ghost" size="sm" disabled={busy === row.id} onclick={() => openSettings(row)}>Open settings</Button>
          {/if}
        </div>
      </li>
    {/each}
  </ul>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
  }
  .summary {
    margin: 0;
    padding: 0 var(--alloy-layout-row-pad-x);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .list {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-panel);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .row:first-child {
    border-top: none;
  }
  .icon {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-secondary);
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
  .label {
    font-size: var(--alloy-typography-size-base);
    color: var(--alloy-color-text-primary);
  }
  .tag {
    padding: 0 var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-full);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }
  .tag.required {
    border-color: var(--alloy-color-accent-primary);
    color: var(--alloy-color-accent-primary);
  }
  .why {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .end {
    display: flex;
    flex: none;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
    white-space: nowrap;
  }
  .pill.ok {
    color: var(--alloy-color-accent-primary);
  }
  .pill.bad {
    color: var(--alloy-color-text-danger, var(--alloy-color-text-secondary));
  }

  /* Phone: the row stacks so the text keeps its width and the button stays a 44px target. */
  @media (max-width: 899px) {
    .row {
      flex-wrap: wrap;
      min-height: 44px;
    }
    .text {
      flex-basis: calc(100% - 36px - var(--alloy-spacing-scale-3));
    }
    .end {
      flex-direction: row;
      align-items: center;
      justify-content: space-between;
      width: 100%;
      min-height: 44px;
      padding-left: calc(36px + var(--alloy-spacing-scale-3));
    }
    .end :global(button) {
      min-height: 44px;
      min-width: 44px;
    }
  }
</style>
