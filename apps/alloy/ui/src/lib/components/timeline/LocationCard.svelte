<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { ExternalLink, MapPin } from '$lib/icons';
  import { formatAccuracy, formatCoordinates, mapUrl, parseGeoUri } from '$lib/content/geo';
  import { formatTime } from '$lib/content/format';
  import { openExternal } from '$lib/platform/links';
  import { prefs } from '$lib/stores/prefs.svelte';

  interface Props {
    geoUri: string;
    description: string | null;
    live?: boolean;
    liveUntil?: number | null;
  }
  let { geoUri, description, live = false, liveUntil = null }: Props = $props();

  const point = $derived(parseGeoUri(geoUri));
  const sharing = $derived(live && liveUntil !== null && liveUntil > Date.now());
  // The engine writes "Location: lat, lon" as the plain-text fallback; that is not a name worth showing twice.
  const label = $derived(description && !/^Location: -?\d/.test(description) ? description : null);
</script>

<div class="card">
  <span class="pin" class:live={sharing} aria-hidden="true"><Icon icon={MapPin} size={24} /></span>
  <div class="text">
    <span class="title">{sharing ? 'Sharing live location' : live ? 'Live location ended' : (label ?? 'Location')}</span>
    {#if point}
      <span class="coords">{formatCoordinates(point)}</span>
      {#if point.accuracy !== null}<span class="muted">Accuracy {formatAccuracy(point.accuracy)}</span>{/if}
    {:else}
      <span class="muted">This location could not be read.</span>
    {/if}
    {#if sharing && liveUntil}<span class="muted">Until {formatTime(liveUntil, prefs.timeFormat)}</span>{/if}
  </div>
  {#if point}
    <button type="button" class="open" aria-label="Open in maps" data-tooltip="Open in maps" onclick={() => void openExternal(mapUrl(point))}>
      <Icon icon={ExternalLink} size={20} />
    </button>
  {/if}
</div>

<style>
  .card {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    max-width: min(360px, 100%);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-subtle);
  }
  .pin {
    display: grid;
    flex: none;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
  }
  .pin.live {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-weight: var(--alloy-typography-weight-semibold);
    overflow-wrap: anywhere;
  }
  .coords {
    font-variant-numeric: tabular-nums;
    font-size: var(--alloy-typography-size-sm);
  }
  .muted {
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }
  .open {
    display: grid;
    flex: none;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  @media (hover: hover) {
    .open:hover {
      background: var(--alloy-color-surface-hover);
    }
  }
  @media (max-width: 899px) {
    .open {
      width: 44px;
      height: 44px;
    }
  }
</style>
