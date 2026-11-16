<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Button } from '$lib/components/common';
  import { SettingGroup } from '$lib/components/settings/controls';
  import { copyText } from '$lib/platform/clipboard';
  import { isTauriRuntime } from '$lib/platform/files';

  import { t } from '$lib/i18n';

  /** Newest lines shown in the preview; the full log is what Copy and Save hand over. */
  const PREVIEW_LINES = 40;
  /** Same default the native save dialog offers. */
  const LOG_FILE_NAME = 'alloy-debug.log';

  interface DebugLogs {
    text: string;
    lines: number;
    truncated: boolean;
  }

  let logText = $state('');
  let lineCount = $state(0);
  let truncated = $state(false);
  let busy = $state(false);
  let copied = $state(false);
  let status = $state<string | null>(null);
  let error = $state<string | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const supported = isTauriRuntime();
  const preview = $derived(logText ? logText.split('\n').slice(-PREVIEW_LINES).join('\n') : '');

  async function refresh() {
    if (!supported) return;
    try {
      const result = await invoke<DebugLogs>('debug_logs');
      logText = result.text;
      lineCount = result.lines;
      truncated = result.truncated;
    } catch {
      error = t('debug-load-failed');
    }
  }

  async function copyLogs() {
    error = null;
    try {
      await copyText(logText);
      copied = true;
      if (copiedTimer !== undefined) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 2000);
    } catch {
      error = t('debug-copy-failed');
    }
  }

  async function clearLogs() {
    busy = true;
    status = null;
    error = null;
    try {
      await invoke('debug_clear_logs');
      logText = '';
      lineCount = 0;
      truncated = false;
      copied = false;
      status = t('debug-cleared');
    } catch {
      error = t('debug-load-failed');
    } finally {
      busy = false;
    }
  }

  function download() {
    const url = URL.createObjectURL(new Blob([logText], { type: 'text/plain' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = LOG_FILE_NAME;
    link.click();
    URL.revokeObjectURL(url);
  }

  async function saveLogs() {
    busy = true;
    status = null;
    error = null;
    try {
      if (supported) {
        const saved = await invoke<boolean>('debug_save_logs');
        if (saved) status = t('debug-saved');
      } else {
        download();
        status = t('debug-saved');
      }
    } catch {
      error = t('debug-save-failed');
    } finally {
      busy = false;
    }
  }

  onMount(() => void refresh());
  onDestroy(() => {
    if (copiedTimer !== undefined) clearTimeout(copiedTimer);
  });
</script>

<section class="section">
  <SettingGroup title={t('debug-title')} description={t('debug-description')}>
    {#if !supported}
      <p class="empty">{t('debug-unavailable')}</p>
    {:else if preview}
      <!-- A scrollable log: the tab stop is deliberate so a keyboard user can scroll it, and the
           role plus label tells assistive tech what the region is. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <pre class="log" role="region" aria-live="off" aria-label={t('debug-preview')} tabindex="0">{preview}</pre>
    {:else}
      <p class="empty">{t('debug-empty')}</p>
    {/if}

    <p class="meta">
      {t('debug-line-count', { count: lineCount })}
      {#if truncated}
        <span class="note">{t('debug-truncated')}</span>
      {/if}
    </p>

    <div class="actions">
      <Button variant="ghost" size="sm" disabled={!supported || logText.length === 0} onclick={() => void copyLogs()}>
        {copied ? t('debug-copied') : t('debug-copy')}
      </Button>
      <Button variant="ghost" size="sm" disabled={!supported || logText.length === 0 || busy} onclick={() => void clearLogs()}>
        {t('debug-clear')}
      </Button>
      <Button variant="ghost" size="sm" disabled={!supported || logText.length === 0 || busy} onclick={() => void saveLogs()}>
        {t('debug-save')}
      </Button>
    </div>

    {#if status}
      <p class="status" role="status">{status}</p>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </SettingGroup>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-6);
  }

  .log {
    margin: 0;
    max-height: 18rem;
    overflow: auto;
    padding: var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radius-sm);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-secondary);
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-font-size-meta);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .log:focus-visible {
    outline: 2px solid var(--alloy-color-border-prominent);
    outline-offset: 2px;
  }

  .meta {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }

  .note {
    display: block;
    margin-top: var(--alloy-spacing-scale-1);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
    padding-bottom: var(--alloy-spacing-scale-2);
  }

  .empty {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }

  .status {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .error {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
</style>
