<script module lang="ts">
  import { localFileUrl } from '$lib/platform/files';
  import { fetchMediaPath } from '$lib/platform/saveFile';

  /** Fetch the full file into the local media cache. Returns a URL the webview can load. */
  export async function fetchMediaFile(uri: string, mimeType: string): Promise<string> {
    return localFileUrl(await fetchMediaPath(uri, mimeType));
  }
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Download, X } from '$lib/icons';
  import { errorMessage } from '$lib/stores/errors';
  import { pushBack } from '$lib/platform/backstack';
  import { portal } from '$lib/platform/portal';
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
  let root: HTMLDivElement | undefined = $state();
  let opener: HTMLElement | null = null;

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

  // Android Back closes the viewer.
  $effect(() => {
    if (!open) return;
    return pushBack(() => onclose());
  });

  $effect(() => {
    if (open) {
      opener = document.activeElement as HTMLElement | null;
      void tick().then(() => root?.focus());
    } else {
      opener?.focus?.();
      opener = null;
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    } else if (e.key === 'Tab' && root) {
      // Keep focus inside the viewer.
      const f = root.querySelectorAll<HTMLElement>('button:not(:disabled), video, audio');
      if (f.length === 0) return;
      const first = f[0];
      const last = f[f.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

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

{#if open}
  <div class="viewer" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" bind:this={root} onkeydown={onKey} use:portal>
    <header class="bar">
      <h2 class="name">{title}</h2>
      <p class="status" aria-live="polite">{status ?? ''}</p>
      {#if downloadable}
        <button
          type="button"
          class="tool"
          aria-label="Download"
          data-tooltip="Download"
          onclick={download}
          disabled={saving || src === null}
        >
          <Icon icon={Download} size={20} />
        </button>
      {/if}
      <button type="button" class="tool" aria-label="Close" data-tooltip="Close" onclick={onclose}>
        <Icon icon={X} size={20} />
      </button>
    </header>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="stage" onclick={(e) => e.target === e.currentTarget && onclose()}>
      {#if failed}
        <p class="note" role="alert">This {mediaKind} could not be loaded.</p>
      {:else if src === null}
        <p class="note" aria-live="polite">Loading</p>
      {:else if mediaKind === 'video'}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video controls autoplay src={src} aria-label={title}></video>
      {:else if mediaKind === 'audio'}
        <audio controls src={src} aria-label={title}></audio>
      {:else}
        <img {src} alt={title} />
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Fills the whole window on every platform (portalled out of the virtualised timeline, whose transforms would otherwise confine it); the media keeps its own shape, with no panel behind it. */
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: flex;
    flex-direction: column;
    padding-top: var(--alloy-safe-area-top);
    padding-bottom: var(--alloy-safe-area-bottom);
    background: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
    outline: none;
  }

  .bar {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
  }

  .name {
    flex: 1 1 auto;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    flex: none;
    margin: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }

  .tool {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }

  .tool:hover:not(:disabled) {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  .tool:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .tool:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }

  .stage {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-3);
  }

  img,
  video {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  audio {
    width: min(480px, 100%);
  }

  .note {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    text-align: center;
  }
</style>
