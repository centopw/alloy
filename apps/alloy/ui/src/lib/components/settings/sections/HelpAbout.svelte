<script lang="ts">
  import { Button } from '$lib/components/common';
  import { SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { appVersion } from '$lib/platform/appVersion';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';
  import { copyText } from '$lib/platform/clipboard';
  import { openExternal } from '$lib/platform/links';
  import {
    HOMEBREW_TAP_COMMAND,
    HOMEBREW_UPGRADE_COMMAND,
    type InstallMethod,
  } from '$lib/platform/updates';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import { updatesStore } from '$lib/stores/updates.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { onMount } from 'svelte';

  import { t } from '$lib/i18n';
  const REPOSITORY_URL = 'https://github.com/centopw/alloy';

  function openRepository(e: MouseEvent) {
    e.preventDefault();
    void openExternal(REPOSITORY_URL);
  }

  let version = $state('');
  onMount(() => {
    void appVersion().then((v) => (version = v));
  });
  const info = $derived(updatesStore.info);
  const available = $derived(info?.available ? info : null);
  /**
   * Who installed this copy. The backend detects it from the bundle path; a build that predates
   * the field is treated as the manual copy it behaves like.
   */
  const method = $derived<InstallMethod>(info?.install_method ?? 'manual');
  /** The command line most recently copied, so only that button reads "Copied". */
  let copiedCommand = $state('');
  async function copyCommand(command: string) {
    try {
      await copyText(command);
      copiedCommand = command;
    } catch {
      copiedCommand = '';
    }
  }
  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  const lastChecked = $derived(
    updatesStore.lastCheckedAt === null ? 'Never' : new Date(updatesStore.lastCheckedAt).toLocaleString()
  );

  /** Names the engine transport this UI is talking through. */
  function engineLabel(): string {
    if (typeof window === 'undefined') return 'Unknown';
    if ('__TAURI_INTERNALS__' in window) return 'Tauri IPC';
    if (new URLSearchParams(window.location.search).has('engine')) return 'Bridge WebSocket';
    return 'In-memory mock';
  }
</script>

<section class="section">
  <SettingGroup title={t('help-about-application')}>
    <div class="row">
      <span class="label">{t('help-about-name')}</span>
      <span class="value">{t('brand-name')}</span>
    </div>
    <div class="row">
      <span class="label">{t('help-about-version')}</span>
      <span class="value">{version}</span>
    </div>
    <div class="row">
      <span class="label">{t('help-about-licence')}</span>
      <span class="value">{t('help-about-apache-2-0')}</span>
    </div>
    <div class="row">
      <span class="label">{t('help-about-source')}</span>
      <a class="value link" href={REPOSITORY_URL} onclick={openRepository}>{REPOSITORY_URL}</a>
    </div>
    <div class="row">
      <span class="label">{t('help-about-last-checked')}</span>
      <span class="value">{lastChecked}</span>
    </div>
  </SettingGroup>

  <SettingGroup title={t('help-about-updates')} description="Alloy checks for a new release in the background.">
    <div class="row updates">
      <span class="label">{t('help-about-status')}</span>
      <div class="update">
        <span class="value" role="status">
          {#if updatesStore.checking}
            {t('help-about-checking')}
          {:else if available}
            {t('help-about-version')} {available.version} {t('help-about-is-available')}
          {:else}
            {t('help-about-alloy-is-up-to-date')}
          {/if}
        </span>
        {#if updatesStore.error}
          <p class="update-error" role="alert">{updatesStore.error}</p>
        {/if}
        <!--
          A cask owns the bundle it installed: moving the copy in place would fall outside the
          cask's records, so Homebrew is told to do the upgrade instead. A translocated copy sits on
          a read-only disk image and cannot replace itself at all, so it is told to move first.
          Only the manual copy gets the download-and-install control (in the update notice).
        -->
        {#if available && method === 'homebrew'}
          <p class="update-note">{t('help-about-homebrew-explains')}</p>
          <div class="command">
            <code class="mono">{HOMEBREW_UPGRADE_COMMAND}</code>
            <Button variant="ghost" onclick={() => void copyCommand(HOMEBREW_UPGRADE_COMMAND)}
              >{copiedCommand === HOMEBREW_UPGRADE_COMMAND ? t('common-copied') : t('common-copy')}</Button
            >
          </div>
          <p class="update-note">{t('help-about-homebrew-tap-note')}</p>
          <div class="command">
            <code class="mono">{HOMEBREW_TAP_COMMAND}</code>
            <Button variant="ghost" onclick={() => void copyCommand(HOMEBREW_TAP_COMMAND)}
              >{copiedCommand === HOMEBREW_TAP_COMMAND ? t('common-copied') : t('common-copy')}</Button
            >
          </div>
        {:else if available && method === 'translocated'}
          <p class="update-note">{t('help-about-translocated-explains')}</p>
        {/if}
        <div class="update-actions">
          {#if available}
            <Button variant="ghost" onclick={() => updatesStore.openChangelog()}>{t('help-about-whats-new')}</Button>
          {/if}
          {#if available && method !== 'manual'}
            <!-- After running the command (or moving the app) the version is re-checked here. -->
            <Button
              variant="ghost"
              disabled={updatesStore.checking || updatesStore.busy}
              onclick={() => void updatesStore.check()}>{t('help-about-check-again')}</Button
            >
          {:else}
            <Button
              variant="ghost"
              disabled={updatesStore.checking || updatesStore.busy}
              onclick={() => void updatesStore.check()}>{t('help-about-check-for-updates')}</Button
            >
          {/if}
        </div>
      </div>
    </div>
    <SettingToggle
      label={t('help-about-automatically-check-for-updates')}
      description="Looks for a new release every few hours."
      checked={prefs.autoCheckUpdates}
      onchange={(value) => updatesStore.setAutoCheck(value)}
    />
    {#if caps.autoDownloadUpdates}
      <SettingToggle
        label={t('help-about-download-updates-automatically')}
        description="Fetches new releases in the background so they are ready to install."
        checked={prefs.autoDownloadUpdates}
        disabled={!prefs.autoCheckUpdates}
        disabledReason="Turn on automatic checks first."
        onchange={(value) => updatesStore.setAutoDownload(value)}
      />
    {/if}
  </SettingGroup>

  <SettingGroup title={t('help-about-diagnostics')}>
    <div class="row">
      <span class="label">{t('help-about-app-version')}</span>
      <span class="value mono">{version}</span>
    </div>
    {#if import.meta.env.DEV}
      <div class="row">
        <span class="label">{t('help-about-engine')}</span>
        <span class="value mono">{engineLabel()}</span>
      </div>
    {/if}
  </SettingGroup>

  {#if import.meta.env.DEV}
    <SettingGroup title={t('help-about-developer')}>
      <SettingToggle
        label={t('help-about-developer-hud')}
        description="Shows engine and render diagnostics over the app."
        checked={ui.hudVisible}
        onchange={(v) => (ui.hudVisible = v)}
      />
    </SettingGroup>
  {/if}
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-6);
  }
  .row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-4);
    min-height: var(--alloy-layout-control-height);
    padding: var(--alloy-spacing-scale-2) 0;
    font-size: var(--alloy-font-size-row);
    color: var(--alloy-color-text-primary);
  }
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .label {
    flex: none;
    font-size: var(--alloy-font-size-row);
    color: var(--alloy-color-text-muted);
  }
  .value {
    min-width: 0;
    font-size: var(--alloy-font-size-row);
    font-variant-numeric: tabular-nums;
    text-align: right;
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--alloy-typography-font-family-mono);
  }
  .link {
    color: var(--alloy-color-link);
    text-decoration: none;
  }
  .link:hover {
    text-decoration: underline;
  }
  .link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  /* The label sits on the first line of the value column, like every other row; the actions go
     under the status, right-aligned to the same edge as the values above. */
  .update {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    text-align: right;
  }
  .update-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  .update-actions :global(.variant-ghost) {
    border-color: var(--alloy-color-border-prominent);
  }
  @media (max-width: 899px) {
    .link {
      display: inline-flex;
      align-items: center;
      min-height: var(--alloy-layout-control-height);
    }
  }
  .update-error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-font-size-preview);
    overflow-wrap: anywhere;
  }
  /* Prose reads left-aligned even inside the right-aligned value column. */
  .update-note {
    margin: 0;
    max-width: 42ch;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-font-size-preview);
    text-align: left;
    overflow-wrap: anywhere;
  }
  /* The command is copyable as one token: selecting the line takes the whole command. */
  .command {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    max-width: 100%;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radius-sm);
  }
  .command code {
    min-width: 0;
    font-size: var(--alloy-font-size-preview);
    text-align: left;
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
