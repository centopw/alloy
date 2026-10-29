<script lang="ts">
  import { onMount } from 'svelte';
  import { Button, TextField } from '$lib/components/common';
  import { SettingButtonRow, SettingGroup } from '$lib/components/settings/controls';
  import { client } from '$lib/ipc';
  import type { SessionInfoDto } from '$lib/generated/index.js';
  import { errorMessage, expect } from '$lib/stores/errors';
  import { verification } from '$lib/stores/verification.svelte';
  import { Copy } from '$lib/icons';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let password = $state('');
  let recoveryInput = $state('');
  let confirmedKey = $state(false);
  let info = $state<SessionInfoDto | null>(null);
  let infoError = $state<string | null>(null);
  let copied = $state(false);

  /** Groups an unpadded base64 key into blocks of four for reading aloud. */
  function groupKey(key: string): string {
    return key.match(/.{1,4}/g)?.join(' ') ?? key;
  }

  onMount(() => {
    void verification.refreshRecovery();
    void loadInfo();
  });

  // A verification started from this panel finishes while it is open: re-read the session so the
  // status row and the offer agree instead of leaving a freshly verified device looking unverified.
  $effect(() => {
    if (verification.view.step === 'done') void loadInfo();
  });

  async function loadInfo() {
    try {
      info = expect(await client.invoke({ action: 'GetSessionInfo' }), 'SessionInfo').payload.info;
    } catch (e) {
      infoError = errorMessage(e);
    }
  }

  async function copyKey() {
    if (!info?.ed25519_key) return;
    try {
      await navigator.clipboard.writeText(info.ed25519_key);
      copied = true;
    } catch (e) {
      infoError = errorMessage(e);
    }
  }

  async function enable() {
    const ok = await verification.enableRecovery(password || null);
    if (ok) {
      password = '';
      confirmedKey = false;
    }
  }
  async function recover() {
    if (await verification.recoverWithKey(recoveryInput)) recoveryInput = '';
  }
</script>

<section class="section">
  <SettingGroup title="This session" description="Keys and backup state for this device.">
    {#if infoError}
      <p class="error" role="alert">{infoError}</p>
    {:else if info}
      <div class="row">
        <span class="row-label">Session ID</span>
        <span class="row-value mono">{info.device_id}</span>
      </div>
      <div class="row">
        <span class="row-label">Status</span>
        <span class="row-value">{info.verified ? 'Verified' : 'Not verified'}</span>
      </div>
      <div class="row">
        <span class="row-label">Session key</span>
        <span class="row-value">
          {#if info.ed25519_key}
            <span class="mono key">{groupKey(info.ed25519_key)}</span>
            <button
              type="button"
              class="icon-button"
              aria-label="Copy session key"
              onclick={() => void copyKey()}
            >
              <Copy size={16} />
            </button>
          {:else}
            Unavailable
          {/if}
        </span>
      </div>
      {#if copied}
        <p class="row note" role="status">Session key copied.</p>
      {/if}
      <div class="row">
        <span class="row-label">Cross-signing</span>
        <span class="row-value">{info.cross_signing_ready ? 'Set up' : 'Not set up'}</span>
      </div>
      <div class="row">
        <span class="row-label">Key backup</span>
        <span class="row-value">{info.backup_enabled ? 'Enabled' : 'Disabled'}</span>
      </div>
    {:else}
      <p class="muted">Loading session details...</p>
    {/if}
  </SettingGroup>

  <!-- Only an unverified session is offered verification: showing "Start verification" to a device
       that is already verified tells the user their trust state is still in doubt. Devices.svelte
       gates the same control on the same flag. Nothing renders while the state is unknown. -->
  {#if info && !info.verified}
    <SettingGroup title="Verify this device">
      <SettingButtonRow
        label="This device"
        description="Confirm it is you by comparing emojis with another signed-in device."
        buttonLabel="Start verification"
        onclick={() => {
          onclose();
          void verification.requestVerification();
        }}
      />
    </SettingGroup>
  {/if}

  <SettingGroup title="Recovery key">
    <div class="recovery">
      {#if verification.recoveryKey}
        <p class="muted">
          Save this key somewhere safe. It is the only way to restore your encrypted messages, and
          it will not be shown again.
        </p>
        <code class="key">{verification.recoveryKey}</code>
        <label class="check">
          <input type="checkbox" bind:checked={confirmedKey} />
          I have saved my recovery key
        </label>
        <div>
          <Button disabled={!confirmedKey} onclick={() => verification.clearKey()}>Done</Button>
        </div>
      {:else if verification.recovery === 'Enabled'}
        <p class="muted">Recovery is set up. This device has your keys.</p>
      {:else if verification.recovery === 'Lost'}
        <p class="muted">
          Enter your recovery key to unlock your encrypted history on this device.
        </p>
        <form
          class="form"
          onsubmit={(e) => {
            e.preventDefault();
            void recover();
          }}
        >
          <TextField
            type="password"
            bind:value={recoveryInput}
            placeholder="Recovery key"
            aria-label="Recovery key"
            autocomplete="off"
          />
          <Button type="submit" disabled={!recoveryInput.trim() || verification.busy}>Unlock</Button>
        </form>
      {:else}
        <p class="muted">Set up a recovery key so you never lose your encrypted messages.</p>
        <form
          class="form"
          onsubmit={(e) => {
            e.preventDefault();
            void enable();
          }}
        >
          <TextField
            type="password"
            bind:value={password}
            placeholder="Account password"
            aria-label="Account password"
            autocomplete="current-password"
          />
          <Button type="submit" disabled={verification.busy}>Set up recovery</Button>
        </form>
      {/if}
      {#if verification.error}<p class="error" role="alert">{verification.error}</p>{/if}
    </div>
  </SettingGroup>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .muted {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal);
  }
  /* Content inside a SettingGroup card shares the row gutter so text lines up with row labels. */
  .recovery {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x)
      var(--alloy-spacing-scale-4);
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
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .row-label {
    color: var(--alloy-color-text-secondary);
  }
  .row-value {
    min-width: 0;
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    overflow-wrap: anywhere;
    text-align: right;
  }
  .mono {
    font-family: var(--alloy-typography-font-family-mono);
  }
  .key {
    overflow-wrap: anywhere;
    user-select: all;
  }
  .note {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    justify-content: flex-start;
  }
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .icon-button:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .icon-button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .form {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
  }
  input[type='checkbox']:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  code.key {
    display: block;
    padding: var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-hover);
    font-family: var(--alloy-typography-font-family-mono);
    overflow-wrap: anywhere;
    user-select: all;
    color: var(--alloy-color-text-primary);
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-primary);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }

  /* Phone: rows stack value below label so long keys wrap; the checkbox row gets a 44px hit area. */
  @media (max-width: 899px) {
    .row {
      flex-direction: column;
      align-items: stretch;
      min-height: 56px;
    }
    .row-value {
      justify-content: space-between;
      text-align: left;
    }
    .icon-button {
      width: 44px;
      height: 44px;
    }
    .check {
      min-height: 44px;
    }
    .check input[type='checkbox'] {
      width: 24px;
      height: 24px;
      margin: 0;
    }
  }
</style>
