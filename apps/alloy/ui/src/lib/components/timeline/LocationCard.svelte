<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { ExternalLink, MapPin } from '$lib/icons';
  import { formatAccuracy, formatCoordinates, mapUrl, parseGeoUri } from '$lib/content/geo';
  import { openExternal } from '$lib/platform/links';

  import { t } from '$lib/i18n';
  interface Props {
    /** `TimelineItemContent::Location::geo_uri`, an RFC 5870 `geo:` URI. */
    geoUri: string;
    /** `TimelineItemContent::Location::description`, a network-supplied name or null. */
    description: string | null;
  }
  let { geoUri, description }: Props = $props();

  const point = $derived(parseGeoUri(geoUri));
  // The engine may write a plain-text fallback ("Location: 48.2, 16.37"); coordinates are already
  // shown as numbers, so a description that just repeats them is not worth a second line.
  const label = $derived(description && !/^Location: -?\d/.test(description) ? description : null);
</script>

<div class="card">
  <span class="pin" aria-hidden="true"><Icon icon={MapPin} size={24} /></span>
  <div class="text">
    <span class="title">{label ?? t('location-card-location')}</span>
    {#if point}
      <span class="coords">{formatCoordinates(point)}</span>
      {#if point.accuracy !== null}<span class="muted">{t('location-card-accuracy')} {formatAccuracy(point.accuracy)}</span>{/if}
    {:else}
      <span class="muted">{t('location-card-this-location-could-not-be-read')}</span>
    {/if}
  </div>
  {#if point}
    <button type="button" class="open" aria-label={t('location-card-open-in-maps')} data-tooltip={t('location-card-open-in-maps')} onclick={() => void openExternal(mapUrl(point))}>
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
    border-radius: var(--alloy-bubble-radius-nested);
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
