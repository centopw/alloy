<script lang="ts">
  import { onMount } from 'svelte';
  import { SettingGroup, SettingSelect, SettingToggle } from '$lib/components/settings/controls';
  import { requestNotificationPermission } from '$lib/platform/notify';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';
  import {
    notificationSettings,
    type DefaultLevel,
    type MentionRule,
  } from '$lib/stores/notificationSettings.svelte';

  const levelOptions: { value: DefaultLevel; label: string }[] = [
    { value: 'All', label: 'All messages' },
    { value: 'Mentions', label: 'Mentions and keywords' },
  ];

  let permissionError = $state<string | null>(null);

  onMount(() => {
    void notificationSettings.load();
  });

  const settings = $derived(notificationSettings.settings);
  const busy = $derived(notificationSettings.saving > 0);
  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  const notificationTitle = $derived(caps.desktopNotifications ? 'Desktop notifications' : 'Notifications');
  const notificationToggleLabel = $derived(
    caps.desktopNotifications ? 'Show desktop notifications' : 'Show notifications'
  );
  const notificationDisabledReason = $derived(
    caps.desktopNotifications ? 'Turn on desktop notifications first.' : 'Turn on notifications first.'
  );
  async function changeDesktop(enabled: boolean) {
    permissionError = null;
    if (!enabled) {
      prefs.set('notifyDesktop', false);
      return;
    }
    const granted = await requestNotificationPermission().catch(() => false);
    prefs.set('notifyDesktop', granted);
    if (!granted) permissionError = 'Notifications are blocked for Alloy. Allow them in system settings to turn this on.';
  }

  function changeMention(rule: MentionRule, value: boolean) {
    void notificationSettings.setMentionRule(rule, value);
  }
</script>

<div class="section">
  <SettingGroup title="Default notification level" description="Used for rooms that do not have their own setting.">
    {#if settings}
      <SettingSelect
        label="Direct chats"
        value={settings.one_to_one}
        options={levelOptions}
        disabled={busy}
        onchange={(v) => void notificationSettings.setDefault(true, v as DefaultLevel)}
      />
      <SettingSelect
        label="Group chats"
        value={settings.group}
        options={levelOptions}
        disabled={busy}
        onchange={(v) => void notificationSettings.setDefault(false, v as DefaultLevel)}
      />
    {:else if notificationSettings.loading}
      <p class="muted">Loading notification settings…</p>
    {/if}
  </SettingGroup>

  {#if settings}
    <SettingGroup title="Mentions and invitations">
      <SettingToggle
        label="Room mentions"
        description="Notify when someone mentions the whole room with @room."
        checked={settings.room_mention}
        disabled={busy}
        onchange={(v) => changeMention('room_mention', v)}
      />
      <SettingToggle
        label="Mentions of my name"
        description="Notify when someone mentions you by name."
        checked={settings.user_mention}
        disabled={busy}
        onchange={(v) => changeMention('user_mention', v)}
      />
      <SettingToggle
        label="Invitations and calls"
        description="Notify for room invitations and incoming calls."
        checked={settings.invite}
        disabled={busy}
        onchange={(v) => changeMention('invite', v)}
      />
    </SettingGroup>
  {/if}

  <SettingGroup title={notificationTitle}>
    <SettingToggle
      label={notificationToggleLabel}
      description="Show an operating system notification for new messages in rooms that notify."
      checked={prefs.notifyDesktop}
      onchange={(v) => void changeDesktop(v)}
    />
    <SettingToggle
      label="Play a sound"
      description="Play the system notification sound."
      checked={prefs.notifySound}
      disabled={!prefs.notifyDesktop}
      disabledReason={notificationDisabledReason}
      onchange={(v) => prefs.set('notifySound', v)}
    />
    <SettingToggle
      label="Show message preview"
      description="Include the start of the message in the notification. When off, it only says there is a new message."
      checked={prefs.notifyPreview}
      disabled={!prefs.notifyDesktop}
      disabledReason={notificationDisabledReason}
      onchange={(v) => prefs.set('notifyPreview', v)}
    />
    {#if caps.windowFocusOptions}
      <SettingToggle
        label="Only when the window is unfocused"
        description="Skip notifications while Alloy is focused. A room you are viewing never notifies while Alloy is focused."
        checked={prefs.notifyOnlyWhenUnfocused}
        disabled={!prefs.notifyDesktop}
        disabledReason={notificationDisabledReason}
        onchange={(v) => prefs.set('notifyOnlyWhenUnfocused', v)}
      />
    {/if}
  </SettingGroup>

  {#if permissionError}
    <p class="error" role="alert">{permissionError}</p>
  {/if}
  {#if notificationSettings.error}
    <p class="error" role="alert">{notificationSettings.error}</p>
  {/if}
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .muted {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    line-height: var(--alloy-typography-line-height-normal);
  }
</style>
