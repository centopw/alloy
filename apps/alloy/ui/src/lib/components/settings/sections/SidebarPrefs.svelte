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
      description="Show the latest message under each room name."
      checked={prefs.roomListPreviews}
      onchange={(v) => prefs.set('roomListPreviews', v)}
    />
    <SettingToggle
      label={t('sidebar-prefs-group-rooms-into-sections')}
      description="Show Invites, Rooms and People as separate sections."
      checked={prefs.roomListSections}
      onchange={(v) => prefs.set('roomListSections', v)}
    />
    <SettingToggle
      label={t('sidebar-prefs-show-people-as-a-separate-section')}
      checked={prefs.roomListPeople}
      disabled={!prefs.roomListSections}
      disabledReason="Turn on Group rooms into sections to use this."
      onchange={(v) => prefs.set('roomListPeople', v)}
    />
  </SettingGroup>

  <SettingGroup title={t('sidebar-prefs-spaces')}>
    <SettingToggle
      label={t('sidebar-prefs-show-all-rooms-in-home')}
      description="Every room you have joined appears in Home, including rooms inside spaces."
      checked={prefs.homeShowAllRooms}
      onchange={(v) => prefs.set('homeShowAllRooms', v)}
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
