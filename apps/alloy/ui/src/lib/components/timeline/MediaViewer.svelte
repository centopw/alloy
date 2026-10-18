<script module lang="ts">
  import { client } from '$lib/ipc';
  import { localFileUrl } from '$lib/platform/files';

  /** Fetch the full file into the local media cache. Returns a URL the webview can load. */
  export async function fetchMediaFile(uri: string, mimeType: string): Promise<string> {
    const res = await client.getMedia({ kind: 'Remote', data: { uri } }, 'file', {
      mime_type: mimeType,
    });
    if (res.type !== 'MediaRetrieved') throw new Error(`unexpected response: ${res.type}`);
    return localFileUrl(res.payload.local_path);
  }
</script>

<script lang="ts">
  import { Modal } from '../common';

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

  const title = $derived(body || (mediaKind === 'video' ? 'Video' : mediaKind === 'audio' ? 'Audio' : 'Image'));

  // Load the full file only while the viewer is open; reload if the source changes.
  $effect(() => {
    if (!open) return;
    let cancelled = false;
    src = null;
    failed = false;
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

</script>

<Modal {open} {title} {onclose} width="min(960px, 94vw)">
  <div class="viewer">
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
</Modal>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    min-height: 120px;
  }

  img,
  video {
    max-width: 100%;
    max-height: 72vh;
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
  }
</style>
