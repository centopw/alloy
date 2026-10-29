<script lang="ts">
  import { client } from '$lib/ipc';
  import Icon from '$lib/icons/Icon.svelte';
  import { Copy, ExternalLink, X } from '$lib/icons';
  import { copyText } from '$lib/platform/clipboard';
  import { localFileUrl } from '$lib/platform/files';
  import { openExternal } from '$lib/platform/links';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { linkPreviews } from '$lib/stores/linkPreviews.svelte';

  interface Props {
    url: string;
    roomId: string;
  }
  let { url, roomId }: Props = $props();

  let dismissed = $state(false);
  const enabled = $derived(linkPreviews.allowed(roomId));
  const view = $derived(linkPreviews.view(url));
  const preview = $derived(view.preview);
  const image = $derived(preview?.image ?? null);

  let imageSrc = $state<string | null>(null);

  $effect(() => {
    if (enabled && !dismissed) linkPreviews.request(url);
  });

  $effect(() => {
    const mxc = image;
    imageSrc = null;
    if (!mxc) return;
    let cancelled = false;
    void thumbnailUrl(mxc).then((src) => {
      if (!cancelled) imageSrc = src;
    });
    return () => {
      cancelled = true;
    };
  });

  /** Local URL for a link-preview image, or null when the homeserver copy cannot be loaded. */
  async function thumbnailUrl(mxc: string): Promise<string | null> {
    try {
      const res = await client.getMedia({ kind: 'Remote', data: { uri: mxc } }, 'thumbnail', {
        width: 640,
        height: 360,
      });
      if (res.type !== 'MediaRetrieved') return null;
      return await localFileUrl(res.payload.local_path);
    } catch {
      return null;
    }
  }

  const menu = (): MenuItem[] => [
    { id: 'open', label: 'Open link', icon: ExternalLink, onselect: () => void openExternal(url) },
    { id: 'copy', label: 'Copy link address', icon: Copy, onselect: () => void copyText(url) },
    { id: 'hide', label: 'Hide preview', icon: X, onselect: () => (dismissed = true) },
  ];
</script>

{#if enabled && preview && !dismissed}
  <aside class="link-preview" aria-label="Link preview" use:contextmenu={menu}>
    <button type="button" class="open" title={url} onclick={() => void openExternal(url)}>
      {#if imageSrc}
        <img class="image" src={imageSrc} alt="" loading="lazy" />
      {/if}
      <span class="text">
        {#if preview.site_name}
          <span class="site">{preview.site_name}</span>
        {/if}
        {#if preview.title}
          <span class="title">{preview.title}</span>
        {/if}
        {#if preview.description}
          <span class="description">{preview.description}</span>
        {/if}
      </span>
    </button>
    <button type="button" class="dismiss" aria-label="Hide preview" title="Hide preview" onclick={() => (dismissed = true)}>
      <Icon icon={X} size={16} />
    </button>
  </aside>
{/if}

<style>
  .link-preview {
    display: flex;
    align-items: stretch;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-2);
    max-width: 480px;
    min-width: 0;
    width: 100%;
    border-left: 3px solid var(--alloy-color-accent-primary);
    border-radius: var(--alloy-bubble-radius-nested);
    background: var(--alloy-color-surface-subtle);
    overflow: hidden;
  }

  .open {
    display: flex;
    flex: 1;
    min-width: 0;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .image {
    display: block;
    width: 100%;
    max-height: 180px;
    object-fit: cover;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    overflow-wrap: anywhere;
  }

  .site {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }

  .title {
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .description {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .dismiss {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    min-width: var(--alloy-touch-target);
    min-height: var(--alloy-touch-target);
    padding: 0;
    border: none;
    background: none;
    color: var(--alloy-color-text-muted);
    cursor: pointer;
  }

  .dismiss:hover {
    color: var(--alloy-color-text-primary);
  }

  .open:focus-visible,
  .dismiss:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
</style>
