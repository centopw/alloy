<script lang="ts">
  import { onMount } from 'svelte';
  import { SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { getAutostart, setAutostart } from '$lib/platform/autostart';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';

  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  let autostartBusy = $state(false);
  let autostartError = $state<string | null>(null);

  function describe(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  onMount(() => {
    if (!caps.autostart) return;
    let cancelled = false;
    getAutostart()
      .then((enabled) => {
        if (!cancelled) prefs.set('launchAtLogin', enabled);
      })
      .catch((err: unknown) => {
        if (!cancelled) autostartError = `Could not read startup setting: ${describe(err)}`;
      });
    return () => {
      cancelled = true;
    };
  });

  async function changeAutostart(enabled: boolean) {
    prefs.set('launchAtLogin', enabled);
    autostartBusy = true;
    autostartError = null;
    try {
      await setAutostart(enabled);
    } catch (err) {
      prefs.set('launchAtLogin', !enabled);
      autostartError = `Could not change startup setting: ${describe(err)}`;
    } finally {
      autostartBusy = false;
    }
  }
</script>

<div class="section">
  {#if caps.enterToSend}
    <SettingGroup title="Composer">
      <SettingToggle
        label="Send messages with Enter"
        description="When off, Enter inserts a new line and Ctrl/Cmd+Enter sends."
        checked={prefs.enterToSend}
        onchange={(v) => prefs.set('enterToSend', v)}
      />
    </SettingGroup>
  {/if}

  <SettingGroup title="Timeline">
    <SettingToggle
      label="Show typing notifications"
      description="Let others see when you are typing, and show when they are typing."
      checked={prefs.showTyping}
      onchange={(v) => prefs.set('showTyping', v)}
    />
    <SettingToggle
      label="Show a placeholder for removed messages"
      checked={prefs.showRemovedPlaceholder}
      onchange={(v) => prefs.set('showRemovedPlaceholder', v)}
    />
    <SettingToggle
      label="Show join and leave messages"
      checked={prefs.showMembershipEvents}
      onchange={(v) => prefs.set('showMembershipEvents', v)}
    />
    <SettingToggle
      label="Show display name and profile picture changes"
      checked={prefs.showProfileChanges}
      onchange={(v) => prefs.set('showProfileChanges', v)}
    />
    <SettingToggle
      label="Show room name, topic and settings changes"
      checked={prefs.showRoomUpdates}
      onchange={(v) => prefs.set('showRoomUpdates', v)}
    />
    <SettingToggle
      label="Jump to the newest message when you send"
      checked={prefs.jumpToBottomOnSend}
      onchange={(v) => prefs.set('jumpToBottomOnSend', v)}
    />
    <SettingToggle
      label="Autoplay GIFs and videos"
      checked={prefs.autoplayMedia}
      onchange={(v) => prefs.set('autoplayMedia', v)}
    />
  </SettingGroup>

  {#if caps.autostart}
    <SettingGroup title="Startup">
      <SettingToggle
        label="Open Alloy when you log in"
        description="Start Alloy automatically when you sign in to your computer."
        checked={prefs.launchAtLogin}
        disabled={autostartBusy}
        onchange={changeAutostart}
      />
      {#if autostartError}
        <p class="error" role="alert">{autostartError}</p>
      {/if}
    </SettingGroup>
  {/if}
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }

  .error {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
</style>
