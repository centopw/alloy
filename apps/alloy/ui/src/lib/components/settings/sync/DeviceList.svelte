<script lang="ts">
  import { Button, ConfirmDialog } from '$lib/components/common';
  import { formatSyncTime } from './format';
  import type { DeviceView } from './types';

  import { t } from '$lib/i18n';

  interface Props {
    devices: DeviceView[];
    onrevoke: (deviceId: string) => void;
  }

  let { devices, onrevoke }: Props = $props();

  let pending = $state<DeviceView | null>(null);

  function enrolled(device: DeviceView): string {
    return formatSyncTime(device.enrolledAtMs) ?? t('sync-unknown');
  }

  function lastSync(device: DeviceView): string {
    return formatSyncTime(device.lastSyncMs) ?? t('sync-server-never');
  }
</script>

{#if devices.length === 0}
  <p class="empty">{t('sync-devices-empty')}</p>
{:else}
  <table class="devices">
    <thead>
      <tr>
        <th scope="col">{t('sync-devices-label')}</th>
        <th scope="col">{t('sync-devices-enrolled')}</th>
        <th scope="col">{t('sync-devices-last-sync')}</th>
        <th scope="col">{t('sync-devices-epoch')}</th>
        <th scope="col"><span class="sr-only">{t('sync-devices-actions')}</span></th>
      </tr>
    </thead>
    <tbody>
      {#each devices as device (device.id)}
        <tr>
          <td class="label">{device.label || t('sync-unknown')}</td>
          <td>{enrolled(device)}</td>
          <td>{lastSync(device)}</td>
          <td>{device.epoch ?? t('sync-unknown')}</td>
          <td class="actions">
            <Button variant="ghost" size="sm" onclick={() => (pending = device)}>
              {t('sync-devices-revoke')}
            </Button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<ConfirmDialog
  open={pending !== null}
  title={t('sync-devices-revoke-title')}
  message={t('sync-devices-revoke-message', { device: pending?.label ?? '' })}
  confirmLabel={t('sync-devices-revoke-confirm')}
  danger
  onconfirm={() => {
    if (pending) onrevoke(pending.id);
  }}
  onclose={() => (pending = null)}
/>

<style>
  .devices {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .devices th {
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-muted);
    text-align: start;
    white-space: nowrap;
  }

  .devices td {
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    vertical-align: middle;
  }

  .devices .label {
    color: var(--alloy-color-text-primary);
  }

  .devices .actions {
    text-align: end;
  }

  .empty {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
