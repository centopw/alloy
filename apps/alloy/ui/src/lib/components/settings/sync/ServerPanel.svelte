<script lang="ts">
  import { Button, TextField } from '$lib/components/common';
  import {
    SettingButtonRow,
    SettingRow,
    SettingSelect,
  } from '$lib/components/settings/controls';
  import { copyText } from '$lib/platform/clipboard';
  import { formatSyncTime, groupPin, layerLabel } from './format';
  import type { ConnectionTestState, ServerView, SyncMode } from './types';

  import { t } from '$lib/i18n';

  interface Props {
    server: ServerView;
    onconfigure: (input: { mode: SyncMode; url: string; token: string | null }) => void;
    ontestconnection: () => void;
    onsyncnow: () => void;
  }

  let { server, onconfigure, ontestconnection, onsyncnow }: Props = $props();

  let mode = $state<SyncMode>('off');
  let url = $state('');
  let token = $state('');
  let replacingToken = $state(false);
  let copiedPin = $state(false);
  let formError = $state<string | null>(null);
  /** Full widens what leaves the device, so it takes a second, explicit step before it applies. */
  let confirmFull = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  // The engine's answer replaces whatever was typed while the call was in flight.
  $effect(() => {
    mode = server.mode;
    url = server.url;
  });

  const modeOptions = $derived([
    { value: 'off', label: t('sync-server-mode-off') },
    { value: 'prefs', label: t('sync-server-mode-prefs') },
    { value: 'full', label: t('sync-server-mode-full') },
  ]);

  /** The help text names what the mode sends; the labels alone do not. */
  const modeDescription = $derived(
    mode === 'full'
      ? t('sync-server-mode-full-description')
      : mode === 'prefs'
        ? t('sync-server-mode-prefs-description')
        : t('sync-server-mode-description')
  );

  function testDescription(state: ConnectionTestState): string {
    switch (state) {
      case 'running':
        return t('sync-server-test-running');
      case 'ok':
        return t('sync-server-test-ok');
      case 'failed':
        return t('sync-server-test-failed', { message: server.testError ?? '' });
      default:
        return t('sync-server-test-idle');
    }
  }

  const pin = $derived(server.pin ? groupPin(server.pin) : null);
  const lastSync = $derived(formatSyncTime(server.lastSyncMs));
  /**
   * The mirror row exists only when the engine reports the messages layer on. `windowed` is the
   * only thing the status proves about coverage, so its absence claims nothing.
   */
  const mirror = $derived(
    server.layers.find((layer) => layer.key === 'messages' && layer.enabled) ?? null
  );

  function changeMode(value: string) {
    if (value !== 'off' && value !== 'prefs' && value !== 'full') return;
    formError = null;
    mode = value;
    // An already-applied Full needs no second confirmation; widening from another mode does.
    confirmFull = value === 'full' && server.mode !== 'full';
  }

  /** The URL rule is the same for both mirroring modes; Off may keep an old address. */
  function invalidUrl(value: SyncMode): string | null {
    if (value === 'off') return null;
    if (/^https:\/\//i.test(url.trim())) return null;
    return t('sync-server-invalid-url');
  }

  function submit(value: SyncMode) {
    onconfigure({
      mode: value,
      url: url.trim(),
      token: token.trim() ? token.trim() : null,
    });
    token = '';
    replacingToken = false;
  }

  function save() {
    formError = null;
    if (mode === 'full' && server.mode !== 'full') {
      // Full never applies on the silent Save path; it always goes through the panel.
      confirmFull = true;
      return;
    }
    const invalid = invalidUrl(mode);
    if (invalid) {
      formError = invalid;
      return;
    }
    submit(mode);
  }

  function confirmFullMode() {
    formError = invalidUrl('full');
    if (formError) return;
    confirmFull = false;
    submit('full');
  }

  function cancelFull() {
    confirmFull = false;
    formError = null;
    mode = server.mode;
  }

  async function copyPin() {
    if (!server.pin) return;
    try {
      await copyText(server.pin);
      copiedPin = true;
      if (copiedTimer !== undefined) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copiedPin = false), 2000);
    } catch {
      copiedPin = false;
    }
  }
</script>

<div class="server">
  <SettingSelect
    label={t('sync-server-mode')}
    description={modeDescription}
    value={mode}
    options={modeOptions}
    onchange={changeMode}
  />

  <SettingRow label={t('sync-server-url')} description={t('sync-server-url-description')} stacked>
    <TextField
      bind:value={url}
      type="url"
      autocomplete="url"
      placeholder={t('sync-server-url-placeholder')}
      invalid={!!formError}
      aria-label={t('sync-server-url')}
    />
  </SettingRow>

  <SettingRow label={t('sync-server-token')} description={t('sync-server-token-hint')} stacked>
    {#if server.tokenStored && !replacingToken}
      <div class="token-stored">
        <span class="stored">{t('sync-server-token-stored')}</span>
        <Button variant="ghost" size="sm" onclick={() => (replacingToken = true)}>
          {t('sync-server-token-replace')}
        </Button>
      </div>
    {:else}
      <TextField
        bind:value={token}
        type="password"
        autocomplete="off"
        placeholder={t('sync-server-token-placeholder')}
        aria-label={t('sync-server-token')}
      />
    {/if}
  </SettingRow>

  {#if !confirmFull}
    <div class="save-row">
      <Button variant="primary" onclick={save}>{t('sync-server-save')}</Button>
      {#if formError}
        <p class="error" role="alert">{formError}</p>
      {/if}
    </div>
  {/if}

  {#if confirmFull}
    <div
      class="full-confirm"
      role="group"
      aria-label={t('sync-server-full-confirm-title')}
    >
      <p class="confirm-title">{t('sync-server-full-confirm-title')}</p>
      <p class="confirm-intro">{t('sync-server-full-confirm-intro')}</p>
      <ul class="confirm-list">
        <li>{t('sync-server-full-confirm-ciphertext')}</li>
        <li>{t('sync-server-full-confirm-loss')}</li>
        <li>{t('sync-server-full-confirm-revoke')}</li>
        <li>{t('sync-server-full-confirm-media')}</li>
      </ul>
      <div class="confirm-actions">
        <Button variant="primary" onclick={confirmFullMode}>
          {t('sync-server-full-confirm-apply')}
        </Button>
        <Button variant="ghost" onclick={cancelFull}>
          {t('sync-server-full-confirm-cancel')}
        </Button>
      </div>
      {#if formError}
        <p class="error" role="alert">{formError}</p>
      {/if}
    </div>
  {/if}

  <SettingRow label={t('sync-server-pin')} description={t('sync-server-pin-description')}>
    {#if pin}
      <div class="pin-row">
        <code class="pin">{pin}</code>
        <Button
          variant="ghost"
          size="sm"
          ariaLabel={t('sync-server-copy-pin')}
          tooltip={copiedPin ? t('sync-server-copied') : t('sync-server-copy-pin')}
          onclick={() => void copyPin()}
        >
          {copiedPin ? t('sync-server-copied') : t('sync-server-copy-pin')}
        </Button>
      </div>
    {:else}
      <span class="muted">{t('sync-server-pin-empty')}</span>
    {/if}
  </SettingRow>

  <SettingRow label={t('sync-server-test')} description={testDescription(server.test)}>
    <Button
      variant="secondary"
      size="sm"
      disabled={server.test === 'running' || !url.trim()}
      loading={server.test === 'running'}
      onclick={ontestconnection}
    >
      {t('sync-server-test')}
    </Button>
  </SettingRow>
  {#if server.test === 'failed' && server.testError}
    <p class="error" role="alert">
      {t('sync-server-test-failed', { message: server.testError })}
    </p>
  {/if}

  <SettingRow label={t('sync-server-last-sync')}>
    <span class={lastSync ? 'value' : 'muted'}>
      {lastSync ?? t('sync-server-never')}
    </span>
  </SettingRow>
  {#if server.lastError}
    <p class="error" role="alert">
      {t('sync-server-error', { message: server.lastError })}
    </p>
  {/if}

  {#if mirror}
    <div class="layer">
      <span class="layer-key">{layerLabel(mirror.key)}</span>
      <span class={mirror.windowed ? 'layer-state windowed' : 'layer-state'}>
        {mirror.windowed ? t('sync-layer-windowed') : t('sync-layer-enabled')}
      </span>
    </div>
  {/if}

  <SettingButtonRow
    label={t('sync-server-sync-now')}
    description={t('sync-server-sync-now-description')}
    buttonLabel={server.syncing ? t('sync-server-syncing') : t('sync-server-sync-now')}
    busy={server.syncing}
    onclick={onsyncnow}
  />
</div>

<style>
  .server {
    display: flex;
    flex-direction: column;
  }

  .token-stored,
  .pin-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }

  .stored,
  .muted,
  .value {
    font-size: var(--alloy-font-size-preview);
  }

  .stored,
  .value {
    color: var(--alloy-color-text-secondary);
  }

  .muted {
    color: var(--alloy-color-text-muted);
  }

  .pin {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-font-size-meta);
    color: var(--alloy-color-text-secondary);
    overflow-wrap: anywhere;
    user-select: all;
  }

  .save-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) 0;
  }

  .error {
    margin: 0;
    padding: var(--alloy-spacing-scale-1) 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }

  .full-confirm {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    margin: var(--alloy-spacing-scale-2) 0;
    padding: var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-canvas);
  }

  .confirm-title,
  .confirm-intro,
  .confirm-list {
    margin: 0;
  }

  .confirm-title {
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }

  .confirm-intro,
  .confirm-list {
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .confirm-list {
    padding-left: var(--alloy-spacing-scale-4);
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }

  .confirm-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
  }

  .layer {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) 0;
  }

  .layer-key {
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-font-size-row);
  }

  .layer-state {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-font-size-preview);
    text-align: right;
  }

  .layer-state.windowed {
    color: var(--alloy-color-text-primary);
  }
</style>
