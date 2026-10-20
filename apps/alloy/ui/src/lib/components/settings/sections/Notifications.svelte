<script lang="ts">
  import { onMount } from 'svelte';
  import { SettingGroup, SettingSelect, SettingToggle, SettingButtonRow } from '$lib/components/settings/controls';
  import { requestNotificationPermission } from '$lib/platform/notify';
  import { pushStore } from '$lib/stores/push.svelte';
  import { openExternal } from '$lib/platform/links';
  import { Icon, ExternalLink } from '$lib/icons';
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
  let pushError = $state<string | null>(null);
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
  const hasDistributors = $derived(
    pushStore.distributors.length > 0 || pushStore.distributor !== null
  );

  const pushToggleDescription = $derived.by(() => {
    if (!pushStore.enabled) {
      return 'Receive notifications for new messages while Alloy is closed.';
    }
    if (pushStore.permission === 'denied') {
      return 'Notification permission is blocked.';
    }
    if (!hasDistributors) {
      return 'No push app installed. Install a UnifiedPush distributor to receive notifications.';
    }
    if (pushStore.error) {
      return `Registration failed: ${pushStore.error}`;
    }
    if (!pushStore.registered) {
      return 'Waiting for the push app…';
    }
    return pushStore.distributor
      ? `Registered with ${pushStore.distributor}.`
      : 'Registered with push app.';
  });

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

  {#if caps.backgroundPush || pushStore.supported}
    <SettingGroup title="When Alloy is closed" description="Receive background notifications via UnifiedPush without Google services.">
      <SettingToggle
        label="Notifications when Alloy is closed"
        description={pushToggleDescription}
        checked={pushStore.enabled}
        disabled={pushStore.busy}
        onchange={(v) => pushStore.toggle(v)}
      />
      {#if hasDistributors}
        <SettingButtonRow
          label="Push app"
          description={pushStore.distributor ?? 'Default push app'}
          buttonLabel="Choose push app"
          onclick={() => pushStore.pick()}
        />
      {:else}
        <div class="push-instructions">
          <p class="push-text">
            Alloy uses UnifiedPush to deliver notifications while closed without relying on Google services. Install a distributor such as ntfy, NextPush, or Conversations.
          </p>
          <a
            href="https://unifiedpush.org/users/distributors/"
            class="push-link"
            onclick={(e) => {
              e.preventDefault();
              void openExternal('https://unifiedpush.org/users/distributors/');
            }}
          >
            <span>Find a UnifiedPush distributor</span>
            <Icon icon={ExternalLink} size={16} />
          </a>
        </div>
      {/if}
      {#if pushStore.enabled && pushStore.permission === 'denied'}
        <div class="push-alert">
          <p class="error" role="alert">
            Notifications are blocked for Alloy. Open Android Settings &gt; Apps &gt; Alloy &gt; Notifications and turn on Allow notifications.
          </p>
        </div>
      {/if}
      {#if pushStore.enabled && pushStore.error}
        <div class="push-alert">
          <p class="error" role="alert">{pushStore.error}</p>
        </div>
      {/if}
    </SettingGroup>
  {/if}

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
  .push-instructions {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .push-text {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal);
  }

  .push-link {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-accent-primary);
    text-decoration: none;
    cursor: pointer;
    min-height: 44px;
  }

  .push-link:hover {
    text-decoration: underline;
  }

  .push-alert {
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x) var(--alloy-spacing-scale-3);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
</style>
