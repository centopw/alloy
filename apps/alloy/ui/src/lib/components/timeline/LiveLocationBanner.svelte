<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { Navigation } from '$lib/icons';
  import { formatTime } from '$lib/content/format';
  import { liveLocation } from '$lib/stores/liveLocation.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';

  const active = $derived(liveLocation.active);
  const room = $derived(active ? roomsStore.rooms.find((r) => r.id === active.roomId) : null);
</script>

{#if active}
  <div class="banner" role="status">
    <span class="icon" aria-hidden="true"><Icon icon={Navigation} size={16} /></span>
    <span class="text">
      Sharing live location{room ? ` in ${roomTitle(room)}` : ''} until {formatTime(active.until, prefs.timeFormat)}
    </span>
    <button type="button" class="stop" onclick={() => void liveLocation.stop()}>Stop</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-height: 40px;
    padding: 0 var(--alloy-spacing-scale-4);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-typography-size-sm);
  }
  .icon {
    flex: none;
    display: grid;
    color: var(--alloy-color-accent-primary);
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stop {
    flex: none;
    min-height: 32px;
    padding: 0 var(--alloy-spacing-scale-3);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-status-error);
    color: var(--alloy-color-text-on-accent);
    font: inherit;
    cursor: pointer;
  }
  @media (max-width: 899px) {
    .banner {
      min-height: 48px;
    }
    .stop {
      min-height: 44px;
    }
  }
</style>
