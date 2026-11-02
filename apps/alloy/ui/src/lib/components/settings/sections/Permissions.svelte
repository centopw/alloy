<script lang="ts">
  import { SettingGroup } from '$lib/components/settings/controls';
  import PermissionsPanel from '../PermissionsPanel.svelte';

  import { t } from '$lib/i18n';</script>

<div class="section">
  <SettingGroup
    title={t('permissions-permissions')}
    description="Everything Alloy asks your system for. Allowed items update here as soon as you change them."
  >
    <PermissionsPanel />
  </SettingGroup>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-5);
  }
</style>
