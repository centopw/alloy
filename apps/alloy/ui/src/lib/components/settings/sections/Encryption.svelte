<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/common';
  import { SettingGroup } from '$lib/components/settings/controls';
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
      <div class="card">
        <div class="row">
          <span class="row-label">Session ID</span>
          <span class="row-value mono">{info.device_id}</span>
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
      </div>
    {:else}
      <p class="muted">Loading session details...</p>
    {/if}
  </SettingGroup>

  <div class="block">
    <h3 class="subtitle">Verify this device</h3>
    <p class="muted">Confirm it is you by comparing emojis with another signed-in device.</p>
    <div>
      <Button
        onclick={() => {
          onclose();
          void verification.requestVerification();
        }}>Start verification</Button
      >
    </div>
  </div>

  <div class="block">
    <h3 class="subtitle">Recovery key</h3>
    {#if verification.recoveryKey}
      <p class="muted">
        Save this key somewhere safe. It is the only way to restore your encrypted messages, and it
        will not be shown again.
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
      <p class="muted">Enter your recovery key to unlock your encrypted history on this device.</p>
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          void recover();
        }}
      >
        <input
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
        <input
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
  .muted {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal);
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
  input[type='password'] {
    flex: 1;
    min-width: 0;
    font: inherit;
    color: var(--alloy-color-text-primary);
    background: var(--alloy-color-surface-subtle);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
  }
  input[type='password']:focus-visible,
  input[type='checkbox']:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  code.key {
    display: block;
    padding: var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-md);
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
