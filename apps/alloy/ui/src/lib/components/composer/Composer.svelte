<script lang="ts">
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { client } from '$lib/ipc';
  import Icon from '$lib/icons/Icon.svelte';
  import { Paperclip, Send, Smile, X } from '$lib/icons';
  import { canPickFiles, pickFile } from '$lib/platform/files';
  import { EMOJI_PICKER } from '$lib/content/emoji';

  interface Props {
    roomId: string;
  }
  let { roomId }: Props = $props();

  let text = $state('');
  let field: HTMLTextAreaElement | undefined = $state();
  let emojiOpen = $state(false);
  let typing = false;
  let typingTimer: ReturnType<typeof setTimeout> | undefined;

  const replying = $derived(timelineStore.replyTo);
  const editing = $derived(timelineStore.editing);
  const canSend = $derived(text.trim().length > 0);

  // Entering edit mode prefills the field with the original text.
  $effect(() => {
    const e = editing;
    if (e && e.content.kind === 'TextMessage') {
      text = e.content.data.body;
      field?.focus();
    }
  });

  // Switching rooms starts with a clean composer.
  $effect(() => {
    void roomId;
    text = '';
    emojiOpen = false;
  });

  function setTyping(on: boolean) {
    if (typing === on) return;
    typing = on;
    void client.setTyping(roomId, on).catch(() => {});
  }

  function onInput() {
    resize();
    if (!text.trim()) return setTyping(false);
    setTyping(true);
    clearTimeout(typingTimer);
    typingTimer = setTimeout(() => setTyping(false), 4000);
  }

  function resize() {
    if (!field) return;
    field.style.height = 'auto';
    field.style.height = `${Math.min(field.scrollHeight, 160)}px`;
  }

  async function submit() {
    if (!canSend) return;
    const body = text;
    text = '';
    clearTimeout(typingTimer);
    setTyping(false);
    await tickResize();
    await timelineStore.send(body);
  }

  async function tickResize() {
    await Promise.resolve();
    resize();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void submit();
    } else if (e.key === 'Escape') {
      if (emojiOpen) emojiOpen = false;
      else if (replying || editing) {
        timelineStore.cancelCompose();
        if (editing) text = '';
      }
    }
  }

  async function attach() {
    const path = await pickFile();
    if (path) await timelineStore.sendFile(path);
  }

  function addEmoji(e: string) {
    text += e;
    emojiOpen = false;
    field?.focus();
    onInput();
  }

  function banner(): string {
    const it = replying ?? editing;
    if (!it) return '';
    const c = it.content;
    return c.kind === 'TextMessage' ? c.data.body : c.kind === 'Media' ? c.data.body || 'Attachment' : '';
  }
</script>

<div class="composer">
  {#if replying || editing}
    <div class="banner">
      <div class="banner-text">
        <strong>{editing ? 'Editing message' : `Replying to ${replying?.sender_display_name ?? replying?.sender}`}</strong>
        <span>{banner()}</span>
      </div>
      <button
        type="button"
        aria-label="Cancel"
        onclick={() => {
          const wasEditing = !!editing;
          timelineStore.cancelCompose();
          if (wasEditing) text = '';
        }}
      >
        <Icon icon={X} size={16} />
      </button>
    </div>
  {/if}

  <div class="bar">
    {#if canPickFiles()}
      <button type="button" class="icon" aria-label="Attach file" title="Attach file" onclick={attach}>
        <Icon icon={Paperclip} size={20} />
      </button>
    {/if}

    <textarea
      bind:this={field}
      bind:value={text}
      rows="1"
      placeholder="Write a message"
      aria-label="Message"
      oninput={onInput}
      onkeydown={onKeydown}
    ></textarea>

    <div class="emoji-wrap">
      <button
        type="button"
        class="icon"
        aria-label="Emoji"
        aria-expanded={emojiOpen}
        title="Emoji"
        onclick={() => (emojiOpen = !emojiOpen)}
      >
        <Icon icon={Smile} size={20} />
      </button>
      {#if emojiOpen}
        <div class="emoji-grid" role="menu">
          {#each EMOJI_PICKER as e}
            <button type="button" role="menuitem" onclick={() => addEmoji(e)}>{e}</button>
          {/each}
        </div>
      {/if}
    </div>

    <button
      type="button"
      class="send"
      aria-label={editing ? 'Save edit' : 'Send'}
      disabled={!canSend}
      onclick={submit}
    >
      <Icon icon={Send} size={20} />
    </button>
  </div>
</div>

<style>
  .composer {
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-4);
    background: var(--alloy-color-surface-canvas);
  }
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    margin-bottom: var(--alloy-spacing-scale-2);
    border-left: 3px solid var(--alloy-color-accent-primary);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-sm);
    font-size: var(--alloy-typography-size-sm);
  }
  .banner-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .banner-text span {
    color: var(--alloy-color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    display: flex;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
  }
  .bar:focus-within {
    border-color: var(--alloy-color-accent-primary);
  }
  textarea {
    flex: 1;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font: inherit;
    padding: 8px 4px;
    max-height: 160px;
    line-height: var(--alloy-typography-line-height-normal);
  }
  .icon,
  .send,
  .banner button {
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    border-radius: var(--alloy-radii-full);
    width: 36px;
    height: 36px;
    display: inline-grid;
    place-items: center;
    cursor: pointer;
    flex: none;
  }
  .banner button {
    width: 28px;
    height: 28px;
  }
  .icon:hover,
  .banner button:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .send {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
  }
  .send:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .emoji-wrap {
    position: relative;
  }
  .emoji-grid {
    position: absolute;
    bottom: 44px;
    right: 0;
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 2px;
    padding: var(--alloy-spacing-scale-2);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    box-shadow: var(--alloy-elevation-md);
    z-index: 5;
  }
  .emoji-grid button {
    border: 0;
    background: transparent;
    font-size: 1.25rem;
    padding: 4px;
    border-radius: var(--alloy-radii-sm);
    cursor: pointer;
  }
  .emoji-grid button:hover {
    background: var(--alloy-color-surface-hover);
  }
</style>
