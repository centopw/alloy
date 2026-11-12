<script lang="ts">
  import { SettingButtonRow, SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { client } from '$lib/ipc';
  import { errorMessage, expect } from '$lib/stores/errors';
  import { prefs } from '$lib/stores/prefs.svelte';

  import { t } from '$lib/i18n';
  let clearing = $state(false);
  let result = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function clearCache() {
    clearing = true;
    result = null;
    error = null;
    try {
      expect(await client.invoke({ action: 'ClearMediaCache' }), 'MediaCacheCleared');
      result = t('security-privacy-cache-cleared');
    } catch (e) {
      error = errorMessage(e);
    } finally {
      clearing = false;
    }
  }
</script>

<section class="section">
  <SettingGroup title={t('security-privacy-privacy')} description={t('security-privacy-privacy-description')}>
    <SettingToggle
      label={t('security-privacy-send-typing-notifications')}
      description={t('security-privacy-send-typing-notifications-description')}
      checked={prefs.sendTyping}
      onchange={(v) => prefs.set('sendTyping', v)}
    />
  </SettingGroup>
  <SettingGroup title={t('security-privacy-storage')} description={t('security-privacy-storage-description')}>
    <SettingButtonRow
      label={t('security-privacy-clear-media-cache')}
      description={t('security-privacy-clear-media-cache-description')}
      buttonLabel={clearing ? t('security-privacy-clearing') : t('security-privacy-clear-cache')}
      busy={clearing}
      onclick={() => void clearCache()}
    />
    {#if result}
      <p class="status" role="status">{result}</p>
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
    gap: var(--alloy-layout-section-gap);
  }
  .status {
    margin: 0;
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .error {
    margin: 0;
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
</style>
