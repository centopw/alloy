<script lang="ts">
  import { onMount } from 'svelte';
  import { SettingButtonRow, SettingGroup, SettingToggle } from '$lib/components/settings/controls';
  import { client } from '$lib/ipc';
  import { formatBytes } from '$lib/content/format';
  import { errorMessage, expect } from '$lib/stores/errors';
  import { prefs } from '$lib/stores/prefs.svelte';

  let clearing = $state(false);
  let result = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function clearCache() {
    clearing = true;
    result = null;
    error = null;
    try {
      const res = expect(await client.invoke({ action: 'ClearMediaCache' }), 'MediaCacheCleared');
      const freed = res.payload.freed_bytes;
      result = freed > 0 ? `Freed ${formatBytes(freed)}.` : 'The media cache was already empty.';
    } catch (e) {
      error = errorMessage(e);
    } finally {
      clearing = false;
    }
  }
  let blocked = $state<string[]>([]);
  let blockedLoaded = $state(false);
  let blockedError = $state<string | null>(null);
  let unblocking = $state<string | null>(null);

  async function loadBlocked() {
    try {
      blocked = expect(await client.listIgnoredUsers(), 'IgnoredUsers').payload.user_ids;
      blockedError = null;
    } catch (e) {
      blockedError = errorMessage(e);
    } finally {
      blockedLoaded = true;
    }
  }

  async function unblock(userId: string) {
    unblocking = userId;
    try {
      expect(await client.unignoreUser(userId), 'UserUnignored');
      await loadBlocked();
    } catch (e) {
      blockedError = errorMessage(e);
    } finally {
      unblocking = null;
    }
  }

  onMount(() => void loadBlocked());
</script>

<section class="section">
  <SettingGroup title="Privacy" description="Control what others can see about you.">
    <SettingToggle
      label="Send typing notifications"
      description="Others in a room can see when you are typing"
      checked={prefs.sendTyping}
      onchange={(v) => prefs.set('sendTyping', v)}
    />
  </SettingGroup>
  <SettingGroup title="Storage" description="Media is cached on this device so it opens faster.">
    <SettingButtonRow
      label="Clear media cache"
      description="Removes cached images, video, audio and files. They download again when you open them."
      buttonLabel={clearing ? 'Clearing...' : 'Clear cache'}
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
  <SettingGroup title="Blocked users" description="You do not see messages from people you block, in any room.">
    {#if blockedError}
      <p class="error" role="alert">{blockedError}</p>
    {/if}
    {#if blocked.length === 0}
      <p class="status">{blockedLoaded ? 'You have not blocked anyone.' : 'Loading'}</p>
    {:else}
      {#each blocked as userId (userId)}
        <SettingButtonRow
          label={userId}
          buttonLabel={unblocking === userId ? 'Unblocking...' : 'Unblock'}
          busy={unblocking === userId}
          onclick={() => void unblock(userId)}
        />
      {/each}
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
