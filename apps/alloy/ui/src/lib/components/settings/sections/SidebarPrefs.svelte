<script lang="ts">
  import { SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';

  import { t } from '$lib/i18n';
  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
</script>
<div class="section">
  <SettingGroup title={caps.sidebarLayout ? t('sidebar-prefs-room-list') : t('sidebar-prefs-chat-list')}>
    <SettingToggle
      label={t('sidebar-prefs-show-message-previews')}
      description={t('sidebar-prefs-show-message-previews-description')}
      checked={prefs.roomListPreviews}
      onchange={(v) => prefs.set('roomListPreviews', v)}
    />
  </SettingGroup>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-6);
  }
</style>
