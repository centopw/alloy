<script lang="ts">
  import { client } from '$lib/ipc';
  import { localFileUrl } from '$lib/platform/files';
  import { formatBytes } from '$lib/content/format';
  import { Download } from '$lib/icons';
  import Icon from '$lib/icons/Icon.svelte';

  interface Props {
    uri: string;
    mimeType: string;
    body: string;
    mediaKind: string;
    sizeBytes: number;
    width?: number | null;
    height?: number | null;
  }
  let { uri, mimeType, body, mediaKind, sizeBytes, width = null, height = null }: Props = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);
  const visual = $derived(mediaKind === 'image' || mediaKind === 'sticker');
  const aspect = $derived(width && height ? `${width} / ${height}` : '4 / 3');

  $effect(() => {
    if (!visual) return;
    let cancelled = false;
    client
      .getMedia({ kind: 'Remote', data: { uri } }, 'thumbnail', {
        width: 480,
        height: 360,
        mime_type: mimeType,
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

  async function download() {
    const res = await client.getMedia({ kind: 'Remote', data: { uri } }, 'file', { mime_type: mimeType });
    if (res.type === 'MediaRetrieved') {
      const { openExternal } = await import('$lib/platform/links');
      await openExternal(await localFileUrl(res.payload.local_path));
    }
  }
</script>

{#if visual && !failed}
  <div class="frame" style="aspect-ratio: {aspect};">
    {#if src}
      <img {src} alt={body} loading="lazy" />
    {:else}
      <div class="placeholder" aria-busy="true"></div>
    {/if}
  </div>
{:else}
  <button class="file" type="button" onclick={download}>
    <span class="file-name">{body || 'Attachment'}</span>
    <span class="file-meta">{formatBytes(sizeBytes)}</span>
    <Icon icon={Download} size={16} label="Download" />
  </button>
{/if}

<style>
  .frame {
    max-width: min(360px, 100%);
    min-width: 120px;
    max-height: 360px;
    border-radius: var(--alloy-radii-md);
    overflow: hidden;
    background: var(--alloy-color-surface-subtle);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .placeholder {
    width: 100%;
    height: 100%;
    min-height: 80px;
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
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
</style>
