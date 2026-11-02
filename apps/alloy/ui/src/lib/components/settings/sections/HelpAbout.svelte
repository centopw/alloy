<script lang="ts">
  import { Button } from '$lib/components/common';
  import { SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { appVersion } from '$lib/platform/appVersion';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';
  import { openExternal } from '$lib/platform/links';
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
        <div class="update-actions">
          {#if available}
            <Button variant="ghost" size="sm" onclick={() => updatesStore.openChangelog()}>{t('help-about-whats-new')}</Button>
          {/if}
          <Button
            variant="ghost"
            size="sm"
            disabled={updatesStore.checking || updatesStore.busy}
            onclick={() => void updatesStore.check()}>{t('help-about-check-for-updates')}</Button
          >
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
    gap: var(--alloy-layout-section-gap);
  }
  .row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-base);
    color: var(--alloy-color-text-primary);
  }
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .label {
    color: var(--alloy-color-text-secondary);
  }
  .value {
    min-width: 0;
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
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
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
  .label {
    flex: none;
  }
  .update-actions :global(.variant-ghost) {
    border-color: var(--alloy-color-border-prominent);
  }
  @media (max-width: 899px) {
    .link {
      display: inline-flex;
      align-items: center;
      min-height: 44px;
    }
  }
  .update-error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-sm);
    overflow-wrap: anywhere;
  }
</style>
