<script module lang="ts">
  import { localFileUrl } from '$lib/platform/files';
  import { fetchMediaPath } from '$lib/platform/saveFile';

  /** Fetch the full file into the local media cache. Returns a URL the webview can load. */
  export async function fetchMediaFile(uri: string, mimeType: string): Promise<string> {
    return localFileUrl(await fetchMediaPath(uri, mimeType));
  }
</script>

<script lang="ts">
  import { Modal } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Download } from '$lib/icons';
  import { errorMessage } from '$lib/stores/errors';
  import { canDownloadMedia, downloadMedia } from '$lib/platform/saveFile';

  interface Props {
    open: boolean;
    onclose: () => void;
    uri: string;
    mimeType: string;
    body: string;
    mediaKind: string;
  }

  let { open, onclose, uri, mimeType, body, mediaKind }: Props = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);
  let saving = $state(false);
  let status = $state<string | null>(null);

  const title = $derived(
    body || (mediaKind === 'video' ? 'Video' : mediaKind === 'audio' ? 'Audio' : 'Image')
  );
  const downloadable = canDownloadMedia();

  // Load the full file only while the viewer is open; reload when the source changes.
  $effect(() => {
    if (!open) return;
    let cancelled = false;
    src = null;
    failed = false;
    status = null;
    fetchMediaFile(uri, mimeType)
      .then((url) => {
        if (!cancelled) src = url;
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });
    return () => {
      cancelled = true;
    };
  });

  async function download() {
    saving = true;
    status = null;
    try {
      if (await downloadMedia(uri, mimeType, title)) status = 'Saved';
    } catch (err) {
      status = errorMessage(err);
    } finally {
      saving = false;
    }
  }
</script>

<Modal {open} {title} {onclose} width="min(960px, 94vw)">
  <div class="viewer">
    {#if downloadable}
      <div class="toolbar">
        <p class="status" aria-live="polite">{status ?? ''}</p>
        <button type="button" class="download" onclick={download} disabled={saving || src === null}>
          <Icon icon={Download} size={20} />
          <span>{saving ? 'Saving' : 'Download'}</span>
        </button>
      </div>
    {/if}
    <div class="stage">
      {#if failed}
        <p class="status" role="alert">This {mediaKind} could not be loaded.</p>
      {:else if src === null}
        <p class="status" aria-live="polite">Loading</p>
      {:else if mediaKind === 'video'}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video controls src={src} aria-label={title}></video>
      {:else if mediaKind === 'audio'}
        <audio controls src={src} aria-label={title}></audio>
      {:else}
        <img {src} alt={title} />
      {/if}
    </div>
  </div>
</Modal>

<style>
  /* Fixed height bounds the media: the stage fills what is left after the toolbar. */
  .viewer {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    height: min(72vh, 680px);
    min-height: 120px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-3);
  }

  .toolbar .status {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .download {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-height: 36px;
    padding: 0 var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }

  .download:hover:not(:disabled) {
    background: var(--alloy-color-surface-hover);
  }

  .download:disabled {
    color: var(--alloy-color-text-secondary);
    cursor: default;
  }

  .download:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }

  .stage {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  img,
  video {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
  }

  audio {
    width: min(480px, 100%);
  }

  .status {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    text-align: center;
  }

  /* Phones: the Modal is a bottom sheet capped at 90dvh; leave room for its header and the toolbar. */
  @media (max-width: 899px) {
    .viewer {
      height: calc(90dvh - 9rem);
    }

    .download {
      min-height: var(--alloy-touch-target);
      padding: 0 var(--alloy-spacing-scale-4);
    }
  }
</style>
