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
    <SettingToggle
      label={t('sidebar-prefs-group-chats-into-sections')}
      description={t('sidebar-prefs-group-chats-into-sections-description')}
      checked={prefs.roomListSections}
      onchange={(v) => prefs.set('roomListSections', v)}
    />
    <SettingToggle
      label={t('sidebar-prefs-show-direct-messages-as-a-separate-section')}
      checked={prefs.roomListPeople}
      disabled={!prefs.roomListSections}
      disabledReason={t('sidebar-prefs-group-chats-required')}
      onchange={(v) => prefs.set('roomListPeople', v)}
    />
  </SettingGroup>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
</style>
