<script lang="ts">
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { client } from '$lib/ipc';
  import { formatBytes } from '$lib/content/format';
  import { localFileUrl } from '$lib/platform/files';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewerItemsFrom, indexOfEvent } from '$lib/content/mediaItems';
  import { fetchMediaFile } from '$lib/content/mediaFetch';
  import MediaViewer from './MediaViewer.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Download } from '$lib/icons';

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
    /** Saves a file row through the caller's download path; omitted for callers that cannot save. */
    ondownload?: (() => void) | null;
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
    ondownload = null,
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
  // Bumped by Retry: the fetch effect re-runs for the same source.
  let attempt = $state(0);
  function retryLoad() {
    attempt += 1;
  }
  // Only a picture or a video gets the visible retry: an audio or file row already names its item.
  const loadFailed = $derived(openable && failed);
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
    // Reading `attempt` makes a retry re-run the fetch for the same source.
    const generation = attempt;
    void generation;
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
    <img {src} alt={body} loading="lazy" onerror={() => (failed = true)} />
  {:else}
    <div class="placeholder" aria-busy="true"></div>
  {/if}
{/snippet}

{#if loadFailed}
  <div class="frame frame-error" style="min-height: 120px;" role="group" aria-label="Media did not load">
    <span class="error-text">Couldn't load this {mediaKind}.</span>
    <button type="button" class="retry" onclick={retryLoad}>Retry</button>
  </div>
{:else if hasThumb || inlineVideo}
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
      <span class="file-name" title={body || 'Attachment'}>{body || 'Attachment'}</span>
      <span class="file-meta">{formatBytes(sizeBytes)}</span>
    </button>
  {:else}
    <div class="file">
      <span class="file-copy">
        <span class="file-name" title={body || 'Attachment'}>{body || 'Attachment'}</span>
        <span class="file-meta">{formatBytes(sizeBytes)}</span>
      </span>
      {#if ondownload}
        <button
          type="button"
          class="file-download"
          aria-label="Download {body || 'attachment'}"
          title="Download"
          onclick={ondownload}
        >
          <Icon icon={Download} size={16} />
        </button>
      {/if}
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
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-bubble-radius-nested);
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
    border-radius: var(--alloy-bubble-radius-nested);
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

  /* A file tile carries a name, its size and the save control on one readable row, so it measures
     from its content instead of collapsing to the width of the longest word in a file name. The
     minimum yields to the bubble on a narrow screen rather than spilling out of it. */
  .file {
    display: flex;
    flex-direction: row;
    gap: var(--alloy-spacing-scale-3);
    align-items: center;
    min-width: min(240px, 100%);
    max-width: 100%;
    padding: var(--alloy-spacing-bubble-y) var(--alloy-spacing-bubble-x);
    border: 1px solid var(--file-border, var(--alloy-color-border-subtle));
    border-radius: var(--alloy-bubble-radius-nested);
    background: var(--file-surface, var(--alloy-color-surface-elevated));
    color: inherit;
  }

  .file-copy {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    align-items: flex-start;
    min-width: 0;
  }

  /* A file row keeps its download control visible: the tile is the only affordance on touch. */
  .file-download {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  .file-download:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }

  .file-download:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }

  button.file {
    cursor: pointer;
    text-align: left;
  }

  button.file:hover {
    /* A hover lifts the plate above its resting surface; on an own bubble the owner supplies a
       brighter tint because the resting plate is already 12% of the bubble text. */
    background: var(--file-surface-hover, color-mix(in srgb, currentColor 6%, var(--alloy-color-surface-elevated)));
  }

  button.file:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }

  .file-name {
    display: block;
    max-width: 100%;
    overflow: hidden;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    color: inherit;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-meta {
    font-size: var(--alloy-typography-size-xs);
    color: color-mix(in srgb, currentColor 75%, transparent);
  }

  /* The thumbnail is still being fetched: a sweeping sheen, never a still grey block. */
  .placeholder {
    width: 100%;
    height: 100%;
    background: linear-gradient(
      100deg,
      var(--alloy-color-surface-hover) 30%,
      var(--alloy-color-surface-subtle) 50%,
      var(--alloy-color-surface-hover) 70%
    );
    background-size: 220% 100%;
    animation: media-skeleton 1.4s ease-in-out infinite;
  }
  @keyframes media-skeleton {
    0% {
      background-position: 220% 0;
    }
    100% {
      background-position: -220% 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .placeholder {
      animation: none;
    }
  }
  .frame-error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    text-align: center;
  }
  .retry {
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font: inherit;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-3);
    cursor: pointer;
  }
  .retry:hover {
    background: var(--alloy-color-surface-hover);
  }
  .retry:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
