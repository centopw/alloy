<script lang="ts">
  import type { TimelineItemDto } from '$lib/generated';
  import { client } from '$lib/ipc';
  import Icon from '$lib/icons/Icon.svelte';
  import { Play } from '$lib/icons';
  import { localFileUrl } from '$lib/platform/files';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { indexOfEvent, viewerItemsFrom, type MediaItem } from '$lib/content/mediaItems';
  import MediaViewer from './MediaViewer.svelte';

  interface Props {
    items: readonly TimelineItemDto[];
  }
  let { items }: Props = $props();

  /** Tiles shown; a longer album ends with a "+N" tile. */
  const SHOWN = 4;
  const shown = $derived(items.slice(0, SHOWN));
  const extra = $derived(items.length - SHOWN);

  let viewerItems = $state<MediaItem[]>([]);
  let viewerIndex = $state(0);
  let viewerOpen = $state(false);

  function open(item: TimelineItemDto) {
    const all = viewerItemsFrom(timelineStore.items);
    const index = indexOfEvent(all, item.event_id, item.content.kind === 'Media' ? item.content.data.uri : undefined);
    if (index < 0) return;
    viewerItems = all;
    viewerIndex = index;
    viewerOpen = true;
  }

  /** The thumbnail source of a tile: the picture itself, or a video's poster. */
  function sourceOf(item: TimelineItemDto): { uri: string; mime: string } | null {
    if (item.content.kind !== 'Media') return null;
    const d = item.content.data;
    if (d.media_kind === 'video') return d.thumbnail_uri ? { uri: d.thumbnail_uri, mime: 'image/jpeg' } : null;
    return { uri: d.uri, mime: d.mime_type };
  }

  let urls = $state<Record<string, string>>({});
  $effect(() => {
    let cancelled = false;
    for (const item of shown) {
      const source = sourceOf(item);
      if (!source || urls[item.internal_id]) continue;
      client
        .getMedia({ kind: 'Remote', data: { uri: source.uri } }, 'thumbnail', { width: 480, height: 480, mime_type: source.mime })
        .then(async (res) => {
          if (res.type !== 'MediaRetrieved') return;
          const url = await localFileUrl(res.payload.local_path);
          if (!cancelled) urls[item.internal_id] = url;
        })
        .catch(() => {
          /* the tile keeps its placeholder; the viewer still opens the file */
        });
    }
    return () => {
      cancelled = true;
    };
  });

  function label(item: TimelineItemDto): string {
    if (item.content.kind !== 'Media') return 'Media';
    return item.content.data.media_kind === 'video' ? 'Open video' : 'Open picture';
  }
</script>

<div class="album" data-count={Math.min(items.length, SHOWN)} role="group" aria-label="{items.length} pictures">
  {#each shown as item, i (item.internal_id)}
    <button type="button" class="tile" aria-label={label(item)} onclick={() => open(item)}>
      {#if urls[item.internal_id]}
        <img src={urls[item.internal_id]} alt="" loading="lazy" />
      {:else}
        <span class="placeholder" aria-busy="true"></span>
      {/if}
      {#if item.content.kind === 'Media' && item.content.data.media_kind === 'video'}
        <span class="play" aria-hidden="true"><Icon icon={Play} size={24} /></span>
      {/if}
      {#if extra > 0 && i === SHOWN - 1}
        <span class="more" aria-label="{extra} more">+{extra}</span>
      {/if}
    </button>
  {/each}
</div>

<MediaViewer open={viewerOpen} onclose={() => (viewerOpen = false)} items={viewerItems} index={viewerIndex} />

<style>
  .album {
    display: grid;
    gap: 2px;
    width: min(400px, 100%);
    overflow: hidden;
    border-radius: var(--alloy-radii-lg);
  }
  .album[data-count='2'] {
    grid-template-columns: 1fr 1fr;
    aspect-ratio: 2 / 1;
  }
  .album[data-count='3'],
  .album[data-count='4'] {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
    aspect-ratio: 1 / 1;
  }
  .album[data-count='3'] .tile:first-child {
    grid-row: span 2;
  }
  .tile {
    position: relative;
    display: block;
    min-width: 0;
    min-height: 0;
    padding: 0;
    border: 0;
    background: var(--alloy-color-surface-subtle);
    cursor: pointer;
  }
  .tile:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
  img,
  .placeholder {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--alloy-color-text-on-accent);
    background: var(--alloy-color-surface-active);
    opacity: 0.85;
  }
  .more {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-typography-size-xl);
    font-weight: var(--alloy-typography-weight-semibold);
    opacity: 0.85;
  }
</style>
