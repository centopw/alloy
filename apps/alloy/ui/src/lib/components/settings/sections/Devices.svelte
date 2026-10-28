<script lang="ts">
  import { onMount } from 'svelte';
  import { Button, Modal, TextField } from '$lib/components/common';
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

  // Signing a session out needs the account password (the server asks for it before deleting devices).
  let pendingSignOut = $state<{ ids: string[]; label: string } | null>(null);
  let password = $state('');
  let signOutBusy = $state(false);
  let signOutError = $state<string | null>(null);
  const others = $derived(devices.filter((d) => !d.is_current));

  function askSignOut(ids: string[], label: string) {
    pendingSignOut = { ids, label };
    password = '';
    signOutError = null;
  }

  async function confirmSignOut() {
    if (!pendingSignOut || signOutBusy) return;
    signOutBusy = true;
    signOutError = null;
    try {
      const res = await client.deleteDevices(pendingSignOut.ids, password);
      if (res.type === 'AuthRequired') throw new Error('Enter your account password to sign sessions out.');
      expect(res, 'DevicesDeleted');
      pendingSignOut = null;
      await load();
    } catch (e) {
      signOutError = errorMessage(e);
    } finally {
      signOutBusy = false;
    }
  }
</script>

{#snippet signOutDialog()}
  <Modal open={pendingSignOut !== null} title="Sign out" onclose={() => (pendingSignOut = null)} width="420px">
    <form
      class="signout"
      onsubmit={(e) => {
        e.preventDefault();
        void confirmSignOut();
      }}
    >
      <p>{pendingSignOut?.label}</p>
      <TextField bind:value={password} type="password" aria-label="Account password" placeholder="Account password" autocomplete="current-password" />
      {#if signOutError}<p class="error" role="alert">{signOutError}</p>{/if}
      <div class="actions">
        <Button variant="ghost" type="button" onclick={() => (pendingSignOut = null)}>Cancel</Button>
        <Button variant="danger" type="submit" disabled={signOutBusy || password === ''}>Sign out</Button>
      </div>
    </form>
  </Modal>
{/snippet}

{@render signOutDialog()}
<section class="section">
  <SettingGroup title="This session">
    {#if current}
      <div class="row">
        <span class="row-label">Status</span>
        <span class="row-value">{current.verified ? 'Verified' : 'Not verified'}</span>
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
      <p class="error group-note" role="alert">{error}</p>
      <div class="group-note">
        <Button variant="ghost" size="sm" onclick={() => void load()}>Try again</Button>
      </div>
    {:else if ordered.length === 0}
      <p class="muted">No devices found.</p>
    {:else}
      <ul class="list">
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
            {#if !device.is_current}
              <Button variant="ghost" size="sm" onclick={() => askSignOut([device.device_id], `Sign ${device.display_name ?? device.device_id} out of this account.`)}>
                Sign out
              </Button>
            {/if}
          </li>
        {/each}
      </ul>
      <div class="footer-actions">
        <Button variant="ghost" size="sm" onclick={() => void load()}>Refresh</Button>
        {#if others.length > 0}
          <Button variant="danger" size="sm" onclick={() => askSignOut(others.map((d) => d.device_id), `Sign ${others.length} other ${others.length === 1 ? 'session' : 'sessions'} out of this account.`)}>
            Sign out all other sessions
          </Button>
        {/if}
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
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .device {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
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
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-base);
    color: var(--alloy-color-text-primary);
  }
  .row-label {
    color: var(--alloy-color-text-secondary);
  }
  .row-value {
    text-align: right;
  }
  .muted,
  .group-note {
    margin: 0;
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
  }
  .muted {
    color: var(--alloy-color-text-muted);
  }
  .group-note {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
  .error.group-note {
    color: var(--alloy-color-status-error);
  }
  .signout {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
  }
  .signout p {
    margin: 0;
  }
  .actions,
  .footer-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  .footer-actions {
    justify-content: space-between;
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x)
      var(--alloy-spacing-scale-4);
  }
</style>
