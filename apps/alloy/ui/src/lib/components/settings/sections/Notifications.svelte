<script lang="ts">
  import { onMount } from 'svelte';
  import { SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { requestNotificationPermission } from '$lib/platform/notify';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';
  import { permissionsStore } from '$lib/stores/permissions.svelte';

  import { t } from '$lib/i18n';
  interface Props {
    onOpenPermissions?: () => void;
  }
  let { onOpenPermissions }: Props = $props();

  let permissionError = $state<string | null>(null);
  onMount(() => permissionsStore.watch());

  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  const notificationRow = $derived(permissionsStore.rows.find((r) => r.id === 'notifications'));
  const blocked = $derived(notificationRow?.state === 'denied' || permissionError !== null);
  const groupTitle = $derived(
    caps.desktopNotifications ? t('notifications-desktop-notifications') : t('notifications-notifications')
  );
  const toggleLabel = $derived(
    caps.desktopNotifications ? t('notifications-show-desktop-notifications') : t('notifications-show-notifications')
  );
  const disabledReason = $derived(
    caps.desktopNotifications ? t('notifications-turn-on-desktop-notifications-first') : t('notifications-turn-on-notifications-first')
  );

  async function changeDesktop(enabled: boolean) {
    permissionError = null;
    if (!enabled) {
      prefs.set('notifyDesktop', false);
      return;
    }
    const granted = await requestNotificationPermission().catch(() => false);
    prefs.set('notifyDesktop', granted);
    if (!granted) permissionError = t('notifications-blocked-description');
  }
</script>

<div class="section">
  <SettingGroup title={groupTitle} description={t('notifications-delivery-description')}>
    <SettingToggle
      label={toggleLabel}
      description={t('notifications-show-description')}
      checked={prefs.notifyDesktop}
      onchange={(v) => void changeDesktop(v)}
    />
    {#if blocked}
      <div class="notice">
        <p role="status">{permissionError ?? t('notifications-blocked-description')}</p>
        {#if onOpenPermissions}
          <button type="button" class="permissions-link" onclick={onOpenPermissions}>{t('notifications-open-permissions')}</button>
        {/if}
      </div>
    {/if}
    <SettingToggle
      label={t('notifications-play-a-sound')}
      description={t('notifications-play-a-sound-description')}
      checked={prefs.notifySound}
      disabled={!prefs.notifyDesktop}
      disabledReason={disabledReason}
      onchange={(v) => prefs.set('notifySound', v)}
    />
    <SettingToggle
      label={t('notifications-show-message-preview')}
      description={t('notifications-show-message-preview-description')}
      checked={prefs.notifyPreview}
      disabled={!prefs.notifyDesktop}
      disabledReason={disabledReason}
      onchange={(v) => prefs.set('notifyPreview', v)}
    />
    {#if caps.windowFocusOptions}
      <SettingToggle
        label={t('notifications-only-when-the-window-is-unfocused')}
        description={t('notifications-only-when-the-window-is-unfocused-description')}
        checked={prefs.notifyOnlyWhenUnfocused}
        disabled={!prefs.notifyDesktop}
        disabledReason={disabledReason}
        onchange={(v) => prefs.set('notifyOnlyWhenUnfocused', v)}
      />
    {/if}
  </SettingGroup>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .notice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .notice p {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .permissions-link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-accent-primary);
    cursor: pointer;
  }
  .permissions-link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
