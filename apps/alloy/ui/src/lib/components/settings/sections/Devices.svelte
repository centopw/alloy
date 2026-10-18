<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/common';
  import { SettingButtonRow, SettingGroup } from '$lib/components/settings/controls';
  import { client } from '$lib/ipc';
  import type { DeviceDto } from '$lib/generated/index.js';
  import { errorMessage, expect } from '$lib/stores/errors';
  import { verification } from '$lib/stores/verification.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let devices = $state<DeviceDto[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  const ordered = $derived(
    [...devices].sort(
      (a, b) =>
        Number(b.is_current) - Number(a.is_current) || (b.last_seen_ts ?? 0) - (a.last_seen_ts ?? 0)
    )
  );
  const current = $derived(devices.find((d) => d.is_current) ?? null);

  const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

  /** Human "last seen" text from a millisecond timestamp. */
  function lastSeen(ts: number | null): string {
    if (ts == null) return 'Unknown';
    const seconds = Math.round((ts - Date.now()) / 1000);
    if (Math.abs(seconds) < 60) return 'Just now';
    const units: [Intl.RelativeTimeFormatUnit, number][] = [
      ['day', 86400],
      ['hour', 3600],
      ['minute', 60]
    ];
    for (const [unit, size] of units) {
      if (Math.abs(seconds) >= size) return relative.format(Math.round(seconds / size), unit);
    }
    return 'Just now';
  }

  onMount(() => {
    void load();
  });

  async function load() {
    loading = true;
    error = null;
    try {
      devices = expect(await client.invoke({ action: 'ListDevices' }), 'Devices').payload.devices;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  function verifyThisSession() {
    onclose();
    void verification.requestVerification();
  }
</script>

<section class="section">
  <SettingGroup title="This session">
    {#if current}
      <div class="card">
        <div class="row">
          <span class="row-label">Status</span>
          <span class="row-value">{current.verified ? 'Verified' : 'Not verified'}</span>
        </div>
      </div>
      {#if !current.verified}
        <SettingButtonRow
          label="Verify this session"
          description="Compare emojis with another signed-in device to trust this session."
          buttonLabel="Start verification"
          onclick={verifyThisSession}
        />
      {/if}
    {/if}
  </SettingGroup>

  <SettingGroup title="Signed-in devices" description="Sessions signed in to this account.">
    {#if loading}
      <p class="muted">Loading devices...</p>
    {:else if error}
      <p class="error" role="alert">{error}</p>
      <div>
        <Button variant="ghost" size="sm" onclick={() => void load()}>Try again</Button>
      </div>
    {:else if ordered.length === 0}
      <p class="muted">No devices found.</p>
    {:else}
      <ul class="card list">
        {#each ordered as device (device.device_id)}
          <li class="device">
            <div class="device-head">
              <span class="device-name">{device.display_name ?? device.device_id}</span>
              {#if device.is_current}
                <span class="tag">This device</span>
              {/if}
              <span class="tag" class:warn={!device.verified}>
                {device.verified ? 'Verified' : 'Unverified'}
              </span>
            </div>
            <div class="device-meta">
              <span class="mono">{device.device_id}</span>
              <span>Last seen {lastSeen(device.last_seen_ts)}</span>
              {#if device.last_seen_ip}
                <span class="mono">{device.last_seen_ip}</span>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
      <div>
        <Button variant="ghost" size="sm" onclick={() => void load()}>Refresh</Button>
      </div>
    {/if}
  </SettingGroup>
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
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .device {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
  }
  .device + .device {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .device-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
  .device-name {
    min-width: 0;
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
    overflow-wrap: anywhere;
  }
  .device-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-3);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .tag {
    padding: 0 var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-hover);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-secondary);
  }
  .tag.warn {
    color: var(--alloy-color-status-error);
  }
  .mono {
    font-family: var(--alloy-typography-font-family-mono);
    overflow-wrap: anywhere;
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
  .row-label {
    color: var(--alloy-color-text-secondary);
  }
  .row-value {
    text-align: right;
  }
  .muted {
    margin: 0;
    color: var(--alloy-color-text-muted);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
</style>
