<script lang="ts">
  import { onMount } from 'svelte';
  import { Button, TextField } from '$lib/components/common';
  import { SettingButtonRow, SettingGroup } from '$lib/components/settings/controls';
  import { client } from '$lib/ipc';
  import type { SessionInfoDto } from '$lib/generated/index.js';
  import { errorMessage, expect } from '$lib/stores/errors';
  import { verification } from '$lib/stores/verification.svelte';
  import { Copy } from '$lib/icons';

  import { t } from '$lib/i18n';
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
  <SettingGroup title={t('encryption-this-session')} description="Keys and backup state for this device.">
    {#if infoError}
      <p class="error" role="alert">{infoError}</p>
    {:else if info}
      <div class="row">
        <span class="row-label">{t('encryption-session-id')}</span>
        <span class="row-value mono">{info.device_id}</span>
      </div>
      <div class="row">
        <span class="row-label">{t('encryption-status')}</span>
        <span class="row-value">{info.verified ? t('encryption-verified') : t('encryption-not-verified')}</span>
      </div>
      <div class="row">
        <span class="row-label">{t('encryption-session-key')}</span>
        <span class="row-value">
          {#if info.ed25519_key}
            <span class="mono key">{groupKey(info.ed25519_key)}</span>
            <button
              type="button"
              class="icon-button"
              aria-label={t('encryption-copy-session-key')}
              onclick={() => void copyKey()}
            >
              <Copy size={16} />
            </button>
          {:else}
            {t('encryption-unavailable')}
          {/if}
        </span>
      </div>
      {#if copied}
        <p class="row note" role="status">{t('encryption-session-key-copied')}</p>
      {/if}
      <div class="row">
        <span class="row-label">{t('encryption-cross-signing')}</span>
        <span class="row-value">{info.cross_signing_ready ? t('encryption-set-up') : t('encryption-not-set-up')}</span>
      </div>
      <div class="row">
        <span class="row-label">{t('encryption-key-backup')}</span>
        <span class="row-value">{info.backup_enabled ? t('encryption-enabled') : t('encryption-disabled')}</span>
      </div>
    {:else}
      <p class="muted">{t('encryption-loading-session-details')}</p>
    {/if}
  </SettingGroup>

  <!-- Only an unverified session is offered verification: showing "Start verification" to a device
       that is already verified tells the user their trust state is still in doubt. Devices.svelte
       gates the same control on the same flag. Nothing renders while the state is unknown. -->
  {#if info && !info.verified}
    <SettingGroup title={t('encryption-verify-this-device')}>
      <SettingButtonRow
        label={t('encryption-this-device')}
        description="Confirm it is you by comparing emojis with another signed-in device."
        buttonLabel="Start verification"
        onclick={() => {
          onclose();
          void verification.requestVerification();
        }}
      />
    </SettingGroup>
  {/if}

  <SettingGroup title={t('encryption-recovery-key')}>
    <div class="recovery">
      {#if verification.recoveryKey}
        <p class="muted">
          {t('encryption-save-this-key-somewhere-safe-it-is-the-only-way-')}
        </p>
        <code class="key">{verification.recoveryKey}</code>
        <label class="check">
          <input type="checkbox" bind:checked={confirmedKey} />
          {t('encryption-i-have-saved-my-recovery-key')}
        </label>
        <div>
          <Button disabled={!confirmedKey} onclick={() => verification.clearKey()}>{t('common-done')}</Button>
        </div>
      {:else if verification.recovery === 'Enabled'}
        <p class="muted">{t('encryption-recovery-is-set-up-this-device-has-your-keys')}</p>
      {:else if verification.recovery === 'Lost'}
        <p class="muted">
          {t('encryption-enter-your-recovery-key-to-unlock-your-encrypted')}
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
            placeholder={t('encryption-recovery-key')}
            aria-label={t('encryption-recovery-key')}
            autocomplete="off"
          />
          <Button type="submit" disabled={!recoveryInput.trim() || verification.busy}>{t('encryption-unlock')}</Button>
        </form>
      {:else}
        <p class="muted">{t('encryption-set-up-a-recovery-key-so-you-never-lose-your-enc')}</p>
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
            placeholder={t('encryption-account-password')}
            aria-label={t('encryption-account-password')}
            autocomplete="current-password"
          />
          <Button type="submit" disabled={verification.busy}>{t('encryption-set-up-recovery')}</Button>
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
