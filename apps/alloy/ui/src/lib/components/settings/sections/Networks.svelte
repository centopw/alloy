<script lang="ts">
  import { onMount } from 'svelte';
  import { SettingGroup } from '$lib/components/settings/controls';
  import TelegramAppForm from './TelegramAppForm.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import { session, networkLabel } from '$lib/stores/session.svelte';

  import { t } from '$lib/i18n';

  let error = $state<string | null>(null);

  onMount(() => {
    void load();
  });

  /** The list and the pair are engine state: read both so the section never shows a stale answer. */
  async function load() {
    try {
      await Promise.all([session.loadNetworks(), session.refreshTelegramApp()]);
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section class="section">
  <div class="section-head">
    <div class="section-text">
      <h2 class="section-title">{t('settings-networks-title')}</h2>
      <p class="muted">{t('settings-networks-hint')}</p>
    </div>
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if session.networks.length > 0}
    <ul class="networks">
      {#each session.networks as info (info.network)}
        <li class="network">
          <span class="network-name">{networkLabel(info.network)}</span>
          <span class="network-state">
            {info.ready ? t('settings-networks-ready') : t('settings-networks-needs-setup')}
          </span>
        </li>
      {/each}
    </ul>
  {/if}

  <SettingGroup
    title={t('settings-networks-telegram-app')}
    description={t('settings-networks-telegram-app-description')}
  >
    <TelegramAppForm />
  </SettingGroup>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .section-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--alloy-layout-gutter);
  }
  .section-text {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .section-title {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .muted {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .networks {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
    overflow: hidden;
  }
  .network {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-layout-gutter);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .network:first-child {
    border-top: none;
  }
  .network-name {
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }
  .network-state {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .error {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
</style>
