<script lang="ts">
  import { Button, TextField } from '$lib/components/common';
  import {
    SettingButtonRow,
    SettingRow,
    SettingSelect,
  } from '$lib/components/settings/controls';
  import { copyText } from '$lib/platform/clipboard';
  import { formatSyncTime, groupPin } from './format';
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
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  // The engine's answer replaces whatever was typed while the call was in flight.
  $effect(() => {
    mode = server.mode;
    url = server.url;
  });

  const modeOptions = $derived([
    { value: 'off', label: t('sync-server-mode-off') },
    { value: 'prefs', label: t('sync-server-mode-prefs') },
  ]);

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

  function save() {
    formError = null;
    const trimmed = url.trim();
    if (mode === 'prefs' && !/^https:\/\//i.test(trimmed)) {
      formError = t('sync-server-invalid-url');
      return;
    }
    onconfigure({ mode, url: trimmed, token: token.trim() ? token.trim() : null });
    token = '';
    replacingToken = false;
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
    description={t('sync-server-mode-description')}
    value={mode}
    options={modeOptions}
    onchange={(value) => (mode = value === 'prefs' ? 'prefs' : 'off')}
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

  <div class="save-row">
    <Button variant="primary" onclick={save}>{t('sync-server-save')}</Button>
    {#if formError}
      <p class="error" role="alert">{formError}</p>
    {/if}
  </div>

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
</style>
