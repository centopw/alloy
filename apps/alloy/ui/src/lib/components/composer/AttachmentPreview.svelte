<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronLeft, ChevronRight, Ellipsis, File, Image, Play, Send, Smile, X } from '$lib/icons';
  import { Button, Modal, TextArea } from '../common';
  import { contextMenu, type MenuItem } from '$lib/stores/contextmenu.svelte';
  import { emojiPicker } from '$lib/stores/emojiPicker.svelte';
  import { formatBytes } from '$lib/content/format';
  import { t } from '$lib/i18n';
  import { clampIndex, indexAfterRemoval, moveIndex } from './attachmentPreview';
  import type { StagedPicture } from './stageHelper';

  interface Props {
    open: boolean;
    /** The files waiting in the composer. The preview shows them one at a time, in this order. */
    items: StagedPicture[];
    /** Sends what is staged. An empty caption means the files go out on their own. */
    onsend: (caption: string) => void;
    /** The composer draft, which becomes the caption so the two never disagree about it. */
    initialCaption?: string;
    /** Back to the composer: the files stay staged, nothing is sent. */
    onclose: () => void;
    /** Takes one file out of the batch. */
    onremove: (path: string) => void;
  }
  let { open, items, onsend, onclose, onremove, initialCaption = '' }: Props = $props();

  let caption = $state('');
  let requested = $state(0);
  let area: HTMLTextAreaElement | undefined = $state();
  let emojiButton: HTMLButtonElement | undefined = $state();

  // The position is clamped on the way out, so a batch that shrinks never shows a blank page.
  const shown = $derived(clampIndex(requested, items.length));
  const current = $derived(items[shown]);
  const multiple = $derived(items.length > 1);

  // A fresh batch starts on the first file with the composer draft as its caption — there is one
  // caption for a send, not two that disagree — and the keyboard lands on it so a person who dropped
  // a picture can type over it without reaching for the mouse.
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) {
      requested = 0;
      caption = initialCaption;
      void tick().then(() => area?.focus());
    }
    wasOpen = open;
  });

  function step(delta: number) {
    requested = moveIndex(shown, items.length, delta);
  }

  function removeCurrent() {
    if (!current) return;
    const path = current.path;
    // The composer drops the item from `items`; stay on the slot so the next file takes its place.
    requested = indexAfterRemoval(shown, Math.max(items.length - 1, 0));
    onremove(path);
  }

  function removeEvery() {
    // Snapshot first: the composer replaces the batch on every removal.
    for (const path of items.map((item) => item.path)) onremove(path);
    onclose();
  }

  function menu(e: MouseEvent) {
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.open(
      [
        { id: 'remove-file', label: t('attachment-preview-remove-this'), icon: X, onselect: removeCurrent },
        { id: 'remove-all', label: t('attachment-preview-remove-all'), icon: File, onselect: removeEvery },
      ] satisfies MenuItem[],
      { x: anchor.left, y: anchor.top },
      { anchor, ariaLabel: t('attachment-preview-more') }
    );
  }

  function addEmoji(emoji: string) {
    caption += emoji;
    void tick().then(() => {
      area?.focus();
      area?.setSelectionRange(caption.length, caption.length);
    });
  }

  function toggleEmoji() {
    if (emojiPicker.isOpen) {
      emojiPicker.close();
      return;
    }
    const anchor = emojiButton?.getBoundingClientRect();
    if (!anchor) return;
    emojiPicker.open({ anchor, onpick: addEmoji });
  }

  function send() {
    if (!items.length) return;
    onsend(caption.trim());
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    } else if (e.key === 'ArrowRight' && multiple) {
      e.preventDefault();
      step(1);
    } else if (e.key === 'ArrowLeft' && multiple) {
      e.preventDefault();
      step(-1);
    }
  }

  function subtitle(item: StagedPicture): string {
    const size = typeof item.size === 'number' ? formatBytes(item.size) : '';
    return size ? `${item.name} · ${size}` : item.name;
  }
</script>

<Modal
  {open}
  title={t('attachment-preview-file-count', { count: items.length })}
  onclose={onclose}
  width="640px"
  closable={false}
>
  {#snippet headerStart()}
    <button type="button" aria-label={t('common-close')} onclick={onclose}>
      <Icon icon={X} size={20} />
    </button>
  {/snippet}
  {#snippet headerEnd()}
    <button type="button" aria-label={t('attachment-preview-more')} aria-haspopup="menu" onclick={menu}>
      <Icon icon={Ellipsis} size={20} />
    </button>
  {/snippet}

  {#if current}
    <div class="preview" role="group" aria-label={t('attachment-preview-file')}>
      {#if current.preview && current.kind === 'video'}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video
          src={`${current.preview}#t=0.1`}
          muted
          playsinline
          controls
          preload="metadata"
          aria-label={current.name}
          onloadedmetadata={(e) => {
            const v = e.currentTarget;
            if (Number.isFinite(v.duration) && v.duration > 0) v.currentTime = Math.min(0.1, v.duration / 2);
          }}
          onerror={() => (current.preview = null)}
        ></video>
      {:else if current.preview}
        <img src={current.preview} alt={t('attachment-preview-picture-alt', { name: current.name })} onerror={() => (current.preview = null)} />
      {:else}
        <div class="card">
          <span class="card-icon" aria-hidden="true">
            <Icon icon={current.kind === 'video' ? Play : current.kind === 'image' ? Image : File} size={24} />
          </span>
        </div>
      {/if}

      {#if multiple}
        <div class="pager">
          <button type="button" class="round" aria-label={t('attachment-preview-previous')} onclick={() => step(-1)} disabled={shown === 0}>
            <Icon icon={ChevronLeft} size={20} />
          </button>
          <span class="counter" aria-live="polite">{shown + 1} / {items.length}</span>
          <button type="button" class="round" aria-label={t('attachment-preview-next')} onclick={() => step(1)} disabled={shown >= items.length - 1}>
            <Icon icon={ChevronRight} size={20} />
          </button>
        </div>
      {/if}
    </div>

    <p class="name" title={current.name}>{subtitle(current)}</p>
  {/if}

  <div class="caption-row">
    <div class="caption">
      <TextArea
        bind:el={area}
        bind:value={caption}
        rows={1}
        maxRows={4}
        placeholder={t('attachment-preview-add-a-caption')}
        aria-label={t('attachment-preview-add-a-caption')}
        onkeydown={onKeydown}
      />
    </div>
    <button
      type="button"
      class="round"
      bind:this={emojiButton}
      aria-label={t('composer-insert-emoji')}
      onclick={toggleEmoji}
    >
      <Icon icon={Smile} size={20} />
    </button>
    <Button variant="primary" size="sm" icon={Send} ariaLabel={t('composer-send-attachment')} onclick={send} disabled={items.length === 0} />
  </div>
</Modal>

<style>
  .preview {
    position: relative;
    display: grid;
    place-items: center;
    min-height: 180px;
    border-radius: var(--alloy-radii-xl);
    overflow: hidden;
    background: var(--alloy-glass-control);
  }
  .preview img,
  .preview video {
    max-width: 100%;
    max-height: 46vh;
    object-fit: contain;
    display: block;
  }
  .card {
    display: grid;
    place-items: center;
    padding: var(--alloy-layout-gutter);
    color: var(--alloy-color-text-secondary);
  }
  .pager {
    position: absolute;
    inset: auto 0 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--alloy-layout-gutter);
    padding: var(--alloy-spacing-scale-2);
    background: linear-gradient(transparent, var(--alloy-color-scrim));
  }
  .counter {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }
  .name {
    margin: var(--alloy-layout-gutter) 0 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caption-row {
    display: flex;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-layout-gutter);
  }
  /* The field is the shared TextArea; this wrapper only takes the row's remaining width. Scoped
     styles do not reach inside a component, so the sizing lives here rather than on the field. */
  .caption {
    flex: 1 1 auto;
    min-width: 0;
  }
  .round {
    flex: none;
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
    border-radius: var(--alloy-radii-full);
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
  }
  .round:hover {
    background: var(--alloy-glass-control);
  }
  .round:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
