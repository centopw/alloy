<script lang="ts">
  import { client } from '$lib/ipc';
  import { formatBytes } from '$lib/content/format';
  import { localFileUrl } from '$lib/platform/files';
  import { prefs } from '$lib/stores/prefs.svelte';
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
  }: Props = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);
  let viewerOpen = $state(false);

  const visual = $derived(mediaKind === 'image' || mediaKind === 'sticker');
  const video = $derived(mediaKind === 'video');
  const openable = $derived(mediaKind === 'image' || video || mediaKind === 'audio');
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
      onclick={() => (viewerOpen = true)}
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
    <button class="file" type="button" onclick={() => (viewerOpen = true)}>
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
    {uri}
    {mimeType}
    {body}
    {mediaKind}
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
    display: block;
    padding: 0;
    border: 0;
    background: transparent;
    font: inherit;
    color: inherit;
    text-align: inherit;
    cursor: pointer;
  }
  .frame-open:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  img,
  video,
  canvas {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }
  .placeholder {
    width: 100%;
    height: 100%;
    min-height: 80px;
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-subtle);
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    max-width: 100%;
  }
  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .file-meta {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  @media (max-width: 899px) {
    .frame {
      max-width: 100%;
    }
    .file {
      min-height: 44px;
    }
  }
</style>
