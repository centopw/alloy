<script lang="ts">
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { client } from '$lib/ipc';
  import { formatBytes } from '$lib/content/format';
  import { localFileUrl } from '$lib/platform/files';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewerItemsFrom, indexOfEvent } from '$lib/content/mediaItems';
  import MediaViewer, { fetchMediaFile } from './MediaViewer.svelte';

  interface Props {
    uri: string;
    mimeType: string;
    body: string;
    mediaKind: string;
    sizeBytes: number;
    width?: number | null;
    height?: number | null;
    /** Server-provided poster for videos. */
    thumbnailUri?: string | null;
    /** Optional event ID for gallery building. */
    eventId?: string | null;
  }

  let {
    uri,
    mimeType,
    body,
    mediaKind,
    sizeBytes,
    width = null,
    height = null,
    thumbnailUri = null,
    eventId = null,
  }: Props = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);
  let viewerOpen = $state(false);
  let viewerItems = $state<ReturnType<typeof viewerItemsFrom>>([]);
  let viewerIndex = $state(0);

  const visual = $derived(mediaKind === 'image' || mediaKind === 'sticker');
  const video = $derived(mediaKind === 'video');
  // Audio and file items do NOT open the viewer
  const openable = $derived(mediaKind === 'image' || video);
  // Images thumbnail their own media; videos use their poster when one exists.
  const thumbSource = $derived(visual ? uri : video ? thumbnailUri : null);
  const thumbMime = $derived(visual ? mimeType : 'image/jpeg');
  const hasThumb = $derived(thumbSource !== null && !failed);
  const aspect = $derived(width && height ? `${width} / ${height}` : '4 / 3');

  /** GIFs stay on their first frame unless autoplay is on; the viewer still plays them. */
  const still = $derived(visual && mimeType === 'image/gif' && !prefs.autoplayMedia);
  let playUrl = $state<string | null>(null);
  let canvasEl = $state<HTMLCanvasElement | undefined>();
  const inlineVideo = $derived(video && prefs.autoplayMedia && playUrl !== null);

  // Autoplay is best-effort: if the file cannot be fetched, the poster stays and the viewer still opens.
  $effect(() => {
    if (!video || !prefs.autoplayMedia) return;
    let cancelled = false;
    playUrl = null;
    fetchMediaFile(uri, mimeType)
      .then((url) => {
        if (!cancelled) playUrl = url;
      })
      .catch(() => {
        if (!cancelled) playUrl = null;
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const source = src;
    const canvas = canvasEl;
    if (!still || source === null || !canvas) return;
    const frame = new Image();
    frame.onload = () => {
      canvas.width = frame.naturalWidth;
      canvas.height = frame.naturalHeight;
      canvas.getContext('2d')?.drawImage(frame, 0, 0);
    };
    frame.src = source;
  });

  $effect(() => {
    const source = thumbSource;
    if (source === null) return;
    let cancelled = false;
    src = null;
    failed = false;
    client
      .getMedia({ kind: 'Remote', data: { uri: source } }, 'thumbnail', {
        width: 480,
        height: 360,
        mime_type: thumbMime,
      })
      .then(async (res) => {
        if (res.type !== 'MediaRetrieved') throw new Error('unexpected response');
        const url = await localFileUrl(res.payload.local_path);
        if (!cancelled) src = url;
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });
    return () => {
      cancelled = true;
    };
  });

  function openViewer() {
    // Build gallery from timeline when opening
    viewerItems = viewerItemsFrom(timelineStore.items);
    viewerIndex = indexOfEvent(viewerItems, eventId, uri);

    // If not found in gallery, fall back to single-item mode
    if (viewerIndex < 0) {
      viewerItems = [
        {
          eventId: eventId || null,
          uri,
          mimeType,
          body,
          mediaKind,
          sender: '',
          senderName: null,
          timestampMillis: Date.now(),
        },
      ];
      viewerIndex = 0;
    }

    viewerOpen = true;
  }
</script>

{#snippet frameContent()}
  {#if inlineVideo}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video src={playUrl} autoplay muted loop playsinline aria-label={body || 'Video'}></video>
  {:else if src && still}
    <canvas bind:this={canvasEl} aria-label={body || 'Animated image, paused'}></canvas>
  {:else if src}
    <img {src} alt={body} loading="lazy" />
  {:else}
    <div class="placeholder" aria-busy="true"></div>
  {/if}
{/snippet}

{#if hasThumb || inlineVideo}
  {#if openable}
    <button
      class="frame frame-open"
      type="button"
      style="aspect-ratio: {aspect};"
      aria-label="Open {body || mediaKind}"
      onclick={openViewer}
    >
      {@render frameContent()}
    </button>
  {:else}
    <div class="frame" style="aspect-ratio: {aspect};">
      {@render frameContent()}
    </div>
  {/if}
{:else}
  {#if openable}
    <button class="file" type="button" onclick={openViewer}>
      <span class="file-name">{body || 'Attachment'}</span>
      <span class="file-meta">{formatBytes(sizeBytes)}</span>
    </button>
  {:else}
    <div class="file">
      <span class="file-name">{body || 'Attachment'}</span>
      <span class="file-meta">{formatBytes(sizeBytes)}</span>
    </div>
  {/if}
{/if}

{#if openable}
  <MediaViewer
    open={viewerOpen}
    onclose={() => (viewerOpen = false)}
    items={viewerItems}
    index={viewerIndex}
  />
{/if}

<style>
  .frame {
    max-width: min(400px, 100%);
    min-width: 120px;
    max-height: 360px;
    border-radius: var(--alloy-radii-lg);
    overflow: hidden;
  }
  .frame-open {
    cursor: pointer;
  }
  .frame-open:hover {
    opacity: 0.85;
  }
  .frame-open {
    padding: 0;
    border: none;
    background: none;
  }
  button.frame-open:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
    border-radius: var(--alloy-radii-lg);
  }

  canvas {
    width: 100%;
    height: 100%;
  }

  img,
  video {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .file {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    align-items: flex-start;
    padding: var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
  }

  button.file {
    cursor: pointer;
    text-align: left;
  }

  button.file:hover {
    background: var(--alloy-color-surface-hover);
  }

  button.file:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }

  .file-name {
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
    word-break: break-word;
  }

  .file-meta {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-secondary);
  }

  .placeholder {
    width: 100%;
    height: 100%;
    background: linear-gradient(135deg, transparent 48%, var(--alloy-color-text-secondary) 49%, var(--alloy-color-text-secondary) 51%, transparent 52%);
    opacity: 0.1;
  }
</style>
