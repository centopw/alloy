<script module lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { isTauriRuntime } from '$lib/platform/files';
  import { prefs } from '$lib/stores/prefs.svelte';

  /**
   * The chat whose background picker is open, or `null`. It lives in the component's module scope so
   * the room menu (`shell/roomMenus.ts`) and the shell that renders the dialog share one source
   * without a second store.
   */
  let pickerRoomId = $state<string | null>(null);

  /** Opens the per-chat picker for `roomId`; the shell renders it. */
  export function openChatBackgroundPicker(roomId: string): void {
    pickerRoomId = roomId;
  }

  /** The chat whose picker is open, or `null` when it is closed. */
  export function chatBackgroundPickerRoom(): string | null {
    return pickerRoomId;
  }

  export function closeChatBackgroundPicker(): void {
    pickerRoomId = null;
  }

  /**
   * Drops the chat's own picture so it falls back to the default one, and deletes the file when
   * nothing else points at it. The room menu offers this without opening the picker.
   */
  export function removeChatBackground(roomId: string): void {
    const previous = prefs.chatBackgrounds[roomId];
    if (!previous) return;
    prefs.setBackground(roomId, null);
    if (!isTauriRuntime() || prefs.backgroundInUse(previous)) return;
    void invoke('delete_background', { fileName: previous }).catch((err) => {
      console.warn('[backgrounds] could not delete the removed picture', err);
    });
  }
</script>

<script lang="ts">
  // `invoke`, `isTauriRuntime` and `prefs` come from the module script above: one module scope, one
  // import each.
  import Button from '$lib/components/common/Button.svelte';
  import { t } from '$lib/i18n';
  import { blobToBase64 } from '$lib/platform/files';
  import { errorMessage } from '$lib/stores/errors';
  import { BACKGROUND_DEFAULT_KEY, backgroundUrl } from '$lib/stores/prefs.svelte';

  interface Props {
    /** The chat whose picture is edited, or `null` for the default every other chat inherits. */
    roomId: string | null;
    /** Called after every change, so a hosting dialog can close on its own terms. */
    onchange?: () => void;
  }

  let { roomId, onchange }: Props = $props();

  /** Largest picture accepted, matching the shell's own bound. */
  const MAX_BACKGROUND_BYTES = 12 * 1024 * 1024;

  /** Picture formats the shell stores, by the MIME type the file chooser reports. */
  const IMAGE_EXTENSIONS: Readonly<Record<string, string>> = {
    'image/png': 'png',
    'image/jpeg': 'jpg',
    'image/webp': 'webp',
    'image/gif': 'gif',
    'image/avif': 'avif',
    'image/bmp': 'bmp',
  };

  /** A failure worth telling the user about; nothing here fails silently. */
  type BackgroundFailure =
    | { kind: 'not-an-image' }
    | { kind: 'too-large' }
    | { kind: 'desktop-only' }
    | { kind: 'save-failed'; detail: string };

  let failure = $state<BackgroundFailure | null>(null);
  let busy = $state(false);
  let previewSrc = $state<string | null>(null);

  /** The picture this control owns: the chat's own file, or the default when no chat is named. */
  const own = $derived(
    roomId === null
      ? (prefs.chatBackgrounds[BACKGROUND_DEFAULT_KEY] ?? null)
      : (prefs.chatBackgrounds[roomId] ?? null),
  );
  const isDefault = $derived(roomId === null);
  /** The default picture, for the "use for new chats" row of a per-chat picker. */
  const defaultPicture = $derived(prefs.chatBackgrounds[BACKGROUND_DEFAULT_KEY] ?? null);

  $effect(() => {
    const file = own;
    if (!file) {
      previewSrc = null;
      return;
    }
    let live = true;
    void backgroundUrl(file).then((url) => {
      if (live) previewSrc = url;
    });
    return () => {
      live = false;
    };
  });

  const status = $derived.by(() => {
    if (isDefault) {
      return own ? t('chat-background-default-description') : t('chat-background-no-default');
    }
    if (own) return t('chat-background-chat-has-own');
    if (defaultPicture) return t('chat-background-chat-uses-default');
    return t('chat-background-chat-none');
  });

  const failureText = $derived.by(() => {
    if (!failure) return null;
    switch (failure.kind) {
      case 'not-an-image':
        return t('chat-background-error-not-an-image');
      case 'too-large':
        return t('chat-background-error-too-large');
      case 'desktop-only':
        return t('chat-background-error-desktop-only');
      case 'save-failed':
        return t('chat-background-error-save-failed', { detail: failure.detail });
    }
  });

  /** Copies the chosen picture into the app's own media dir and points the pref at the new name. */
  async function onFileChosen(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    // Clearing lets the user pick the same file again after a failure.
    input.value = '';
    if (!file) return;
    const extension = IMAGE_EXTENSIONS[file.type.toLowerCase()];
    if (!extension) {
      failure = { kind: 'not-an-image' };
      return;
    }
    if (file.size > MAX_BACKGROUND_BYTES) {
      failure = { kind: 'too-large' };
      return;
    }
    if (!isTauriRuntime()) {
      failure = { kind: 'desktop-only' };
      return;
    }
    failure = null;
    busy = true;
    try {
      const data = await blobToBase64(file);
      const stored = await invoke<string>('save_background', { data, extension });
      const previous = own;
      prefs.setBackground(roomId, stored);
      if (previous && previous !== stored && !prefs.backgroundInUse(previous)) {
        await discard(previous);
      }
      onchange?.();
    } catch (err) {
      failure = { kind: 'save-failed', detail: errorMessage(err) };
    } finally {
      busy = false;
    }
  }

  /** Removes the picture this control owns; the chat falls back to the default one. */
  async function remove(): Promise<void> {
    if (!own) return;
    failure = null;
    const previous = own;
    if (roomId === null) {
      prefs.setBackground(null, null);
      if (!prefs.backgroundInUse(previous)) await discard(previous);
    } else {
      removeChatBackground(roomId);
    }
    onchange?.();
  }

  /** Makes this chat's picture the default for every chat that has none of its own. */
  async function useForNewChats(): Promise<void> {
    if (!own || defaultPicture === own) return;
    const previous = defaultPicture;
    prefs.setBackground(null, own);
    if (previous && !prefs.backgroundInUse(previous)) await discard(previous);
    onchange?.();
  }

  /** Deletes a stored picture, keeping a shell failure out of the way of the rest of the change. */
  async function discard(fileName: string): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      await invoke('delete_background', { fileName });
    } catch (err) {
      console.warn('[backgrounds] could not delete the replaced picture', err);
    }
  }
</script>

<div class="chat-background" aria-busy={busy || undefined}>
  <div class="preview">
    {#if previewSrc}
      <!-- Decorative: the status line names the picture, so the preview adds nothing to read. -->
      <img src={previewSrc} alt="" />
    {:else}
      <span class="empty">{t('chat-background-no-image')}</span>
    {/if}
  </div>
  <div class="body">
    <p class="status">{status}</p>
    <div class="actions">
      <!-- The label is the visible control and the input inside it is the focusable one, so the
           chooser stays one tab stop with a keyboard-visible focus ring. -->
      <label class="choose" class:disabled={busy}>
        <span>{own ? t('chat-background-change-image') : t('chat-background-choose-image')}</span>
        <input
          class="chooser"
          type="file"
          accept="image/*"
          disabled={busy}
          onchange={(event) => void onFileChosen(event)}
        />
      </label>
      {#if own}
        <Button variant="ghost" size="sm" disabled={busy} onclick={() => void remove()}>
          {t('chat-background-remove')}
        </Button>
      {/if}
    </div>
    {#if failureText}
      <p class="error" role="alert">{failureText}</p>
    {/if}
  </div>
</div>

{#if roomId !== null && own}
  <div class="default-row">
    <span class="label">{t('chat-background-use-for-new-chats')}</span>
    <Button
      variant="ghost"
      size="sm"
      disabled={busy || defaultPicture === own}
      onclick={() => void useForNewChats()}
    >
      {t('chat-background-apply-to-new-chats')}
    </Button>
  </div>
{/if}

<style>
  .chat-background {
    display: flex;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-4);
    padding: var(--alloy-spacing-scale-2) 0;
  }

  .preview {
    flex: none;
    display: grid;
    place-items: center;
    width: 96px;
    height: 64px;
    overflow: hidden;
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
  }

  .preview img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .empty {
    padding: 0 var(--alloy-spacing-scale-2);
    text-align: center;
    font-size: var(--alloy-font-size-meta);
    color: var(--alloy-color-text-muted);
  }

  .body {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }

  .status {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
  }

  /* A ghost-button shape, because the control is a label around the real file input. */
  .choose {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 32px;
    padding: 0 var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-primary);
    cursor: pointer;
  }

  .choose:hover {
    background: var(--alloy-color-surface-hover);
  }

  .choose:focus-within {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  .choose.disabled {
    opacity: 0.6;
    cursor: default;
  }

  /* Touch: the chooser takes the same full-size target as every button on a phone. */
  @media (max-width: 899px) {
    .choose {
      min-height: 44px;
      min-width: 44px;
    }
  }

  /* Visually hidden but still a tab stop, so the label carries the focus ring above. */
  .chooser {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }

  .error {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-status-error);
  }

  .default-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-4);
    padding: var(--alloy-spacing-scale-2) 0;
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .label {
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }
</style>
