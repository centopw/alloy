<script lang="ts">
  import { SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { prefs } from '$lib/stores/prefs.svelte';
</script>

<div class="section">
  <SettingGroup title="Room list">
    <SettingToggle
      label="Show message previews"
      description="Show the latest message under each room name."
      checked={prefs.roomListPreviews}
      onchange={(v) => prefs.set('roomListPreviews', v)}
    />
    <SettingToggle
      label="Group rooms into sections"
      description="Show Invites, Rooms and People as separate sections."
      checked={prefs.roomListSections}
      onchange={(v) => prefs.set('roomListSections', v)}
    />
    <SettingToggle
      label="Show People as a separate section"
      checked={prefs.roomListPeople}
      disabled={!prefs.roomListSections}
      disabledReason="Turn on Group rooms into sections to use this."
      onchange={(v) => prefs.set('roomListPeople', v)}
    />
  </SettingGroup>

  <SettingGroup title="Spaces">
    <SettingToggle
      label="Show all rooms in Home"
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
