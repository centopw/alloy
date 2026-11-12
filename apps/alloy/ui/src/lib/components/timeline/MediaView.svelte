<script lang="ts">
  import { client } from '$lib/ipc';
  import type { MediaSource } from '$lib/generated';
  import { formatBytes } from '$lib/content/format';
  import { localFileUrl } from '$lib/platform/files';
  import { openExternal } from '$lib/platform/links';
  import { prefs } from '$lib/stores/prefs.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Download, Play } from '$lib/icons';

  import { t } from '$lib/i18n';
  interface Props {
    /** The stored media object; its bytes are fetched through `GetMedia`. */
    source: MediaSource;
    mimeType: string;
    /** Caption, or the file name for attachments. */
    body: string;
    /** `image`, `video`, `file` or `sticker`; audio and voice notes use `AudioPlayer`. */
    mediaKind: string;
    sizeBytes: number;
    width?: number | null;
    height?: number | null;
    /** Optional event ID for the caller's own bookkeeping. */
    eventId?: string | null;
    /** Saves a file row through the caller's download path; omitted for callers that cannot save. */
    ondownload?: (() => void) | null;
  }

  let {
    source,
    mimeType,
    body,
    mediaKind,
    sizeBytes,
    width = null,
    height = null,
    ondownload = null,
  }: Props = $props();

  let thumbSrc = $state<string | null>(null);
  let fileSrc = $state<string | null>(null);
  let failed = $state(false);
  let loadingFile = $state(false);
  // Bumped by Retry: the fetch effect re-runs for the same source.
  let attempt = $state(0);

  const visual = $derived(mediaKind === 'image' || mediaKind === 'sticker');
  const video = $derived(mediaKind === 'video');
  const loadFailed = $derived((visual || video) && failed);
  const aspect = $derived(width && height ? `${width} / ${height}` : '4 / 3');
  /** A caption that is only the file name belongs to the attachment row, not to the picture. */
  const caption = $derived(body || t('media-view-attachment'));

  function retryLoad() {
    attempt += 1;
  }

  async function fetchPath(kind: 'thumbnail' | 'file', mime: string | null): Promise<string> {
    const res = await client.invoke({
      action: 'GetMedia',
      params: { source, kind, mime_type: mime },
    });
    if (res.type !== 'Media') throw new Error(`unexpected response: ${res.type}`);
    return await localFileUrl(res.payload.path);
  }

  // Pictures and videos show a cached thumbnail; autoplay videos fetch the file itself.
  $effect(() => {
    const mime = visual ? mimeType : video ? 'image/jpeg' : null;
    // Reading `attempt` makes a retry re-run the fetch for the same source.
    void attempt;
    if (mime === null) return;
    let cancelled = false;
    thumbSrc = null;
    failed = false;
    fetchPath('thumbnail', mime)
      .then((url) => {
        if (!cancelled) thumbSrc = url;
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });
    return () => {
      cancelled = true;
    };
  });

  // Autoplay is best-effort: if the file cannot be fetched, the poster stays and Play still opens it.
  $effect(() => {
    if (!video || !prefs.autoplayMedia) return;
    let cancelled = false;
    fileSrc = null;
    void fetchPath('file', mimeType)
      .then((url) => {
        if (!cancelled) fileSrc = url;
      })
      .catch(() => {
        if (!cancelled) fileSrc = null;
      });
    return () => {
      cancelled = true;
    };
  });

  /** Images open full size; videos play in place when the file is already cached. */
  async function openFullSize() {
    if (visual) {
      try {
        await openExternal(await fetchPath('file', mimeType));
      } catch {
        failed = true;
      }
      return;
    }
    if (fileSrc) return;
    loadingFile = true;
    try {
      fileSrc = await fetchPath('file', mimeType);
    } catch {
      failed = true;
    } finally {
      loadingFile = false;
    }
  }
</script>

{#snippet frameContent()}
  {#if video && fileSrc}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video src={fileSrc} controls autoplay={prefs.autoplayMedia} playsinline aria-label={body || t('media-view-video')}></video>
  {:else if thumbSrc}
    <img src={thumbSrc} alt={body} loading="lazy" onerror={() => (failed = true)} />
  {:else}
    <div class="placeholder" aria-busy="true"></div>
  {/if}
{/snippet}

{#if loadFailed}
  <div class="frame frame-error" style="min-height: 120px;" role="group" aria-label={t('media-view-media-did-not-load')}>
    <span class="error-text">{t('media-view-couldnt-load-this')} {mediaKind}.</span>
    <button type="button" class="retry" onclick={retryLoad}>{t('common-retry')}</button>
  </div>
{:else if visual || video}
  <button
    class="frame frame-open"
    type="button"
    style="aspect-ratio: {aspect};"
    aria-label={t('media-open', { name: body || mediaKind })}
    onclick={() => void openFullSize()}
    disabled={loadingFile}
  >
    {#if video && !fileSrc}
      <span class="play" aria-hidden="true"><Icon icon={Play} size={24} /></span>
    {/if}
    {@render frameContent()}
  </button>
{:else}
  <div class="file">
    <span class="file-copy">
      <span class="file-name" title={caption}>{caption}</span>
      <span class="file-meta">{formatBytes(sizeBytes)}</span>
    </span>
    {#if ondownload}
      <button
        type="button"
        class="file-download"
        aria-label={t('media-download', { name: body || t('media-attachment') })}
        title={t('media-view-download')}
        onclick={ondownload}
      >
        <Icon icon={Download} size={16} />
      </button>
    {/if}
  </div>
{/if}
<style>
  .frame {
    position: relative;
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

  /* A video that has not been fetched yet shows its poster under a play badge. */
  .play {
    position: absolute;
    inset: 50% auto auto 50%;
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-scrim);
    color: var(--alloy-color-text-on-accent);
    translate: -50% -50%;
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

  /* A hover lifts the plate above its resting surface; on an own bubble the owner supplies a
     brighter tint because the resting plate is already 12% of the bubble text. */
  .file:hover {
    background: var(--file-surface-hover, color-mix(in srgb, currentColor 6%, var(--alloy-color-surface-elevated)));
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
