<script lang="ts">
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { threadStore } from '$lib/stores/thread.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { client } from '$lib/ipc';
  import { prefs } from '$lib/stores/prefs.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChartBar, Paperclip, Plus, Send, Smile, X } from '$lib/icons';
  import { canPickFiles, pickFile } from '$lib/platform/files';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { contextMenu, type MenuItem } from '$lib/stores/contextmenu.svelte';
  import { emojiPicker } from '$lib/stores/emojiPicker.svelte';
  import { tick, untrack } from 'svelte';
  import type { RoomMemberDto } from '$lib/generated/index.js';
  import { membersStore, memberLabel } from '$lib/stores/members.svelte';
  import {
    activeMention,
    MENTION_LIMIT,
    mentionOptionId,
    rankMembers,
    reconcileMentions,
    type ActiveMention,
    type MentionRecord
  } from '$lib/stores/mentions';
  import MentionPicker from './MentionPicker.svelte';
  import PollDialog from './PollDialog.svelte';

  interface Props {
    roomId: string;
    /** Set inside a thread panel: sends go into that thread and room reply/edit state is ignored. */
    threadRoot?: string | null;
  }
  let { roomId, threadRoot = null }: Props = $props();
  const pickerId = $props.id();

  let text = $state('');
  let field: HTMLTextAreaElement | undefined = $state();
  let mention = $state<ActiveMention | null>(null);
  let pollOpen = $state(false);
  // Key (`start:query`) of a mention dismissed with Escape; it reopens once the query changes.
  let mentionDismissed = $state<string | null>(null);
  let mentionIndex = $state(0);
  // Mentions picked for the current draft, checked against the text on send.
  let mentionRecords: MentionRecord[] = [];
  let typing = false;
  let typingTimer: ReturnType<typeof setTimeout> | undefined;

  const replying = $derived(threadRoot ? null : timelineStore.replyTo);
  const editing = $derived(threadRoot ? null : timelineStore.editing);
  const composing = $derived(!!(replying || editing));
  const canSend = $derived(text.trim().length > 0);
  const roomName = $derived.by(() => {
    const active = roomsStore.active;
    return active && active.id === roomId ? roomTitle(active) : null;
  });
  // Textarea placeholders wrap instead of truncating, so long room names are cut here.
  // Desktop with Enter as newline states the send shortcut; phones have a Send button and need no hint.
  const hintSend = $derived(!viewport.isPhone && !prefs.enterToSend);
  // Phones: the placeholder must stay on one line, so only the room name fits when the whole string is short.
  // The full name stays in aria-label.
  const phoneName = $derived(roomName ? `Message ${roomName}` : 'Message');
  const placeholder = $derived.by(() => {
    if (viewport.isPhone) return phoneName.length < 18 ? phoneName : 'Message';
    const base = roomName ? `Message ${roomName.length > 28 ? `${roomName.slice(0, 27)}…` : roomName}` : 'Message';
    return hintSend ? `${base}. Ctrl+Enter or Cmd+Enter to send` : base;
  });
  const mentionKey = (m: ActiveMention) => `${m.start}:${m.query}`;
  const candidates = $derived(
    mention ? rankMembers(membersStore.membersOf(roomId), mention.query, MENTION_LIMIT) : []
  );
  const pickerOpen = $derived(
    mention !== null && candidates.length > 0 && mentionKey(mention) !== mentionDismissed
  );
  const highlighted = $derived(Math.min(mentionIndex, Math.max(candidates.length - 1, 0)));

  // Entering edit mode prefills the field with the original text.
  $effect(() => {
    const e = editing;
    if (e && e.content.kind === 'TextMessage') {
      text = e.content.data.body;
      field?.focus();
    }
  });

  // Switching rooms starts with a clean composer and loads that room's members for mentions.
  $effect(() => {
    void roomId;
    text = '';
    untrack(() => emojiPicker.close());
    mention = null;
    mentionDismissed = null;
    mentionRecords = [];
    // Without loaded members the picker stays empty; sending is unaffected.
    // Untracked: the cache read inside must not make members arriving later wipe the draft.
    untrack(() => void membersStore.loadMembers(roomId).catch(() => {}));
  });

  // Typing notices follow the sendTyping preference. When it is off nothing is sent, and any
  // local typing timer or flag is dropped without a network call.
  function setTyping(on: boolean) {
    if (!prefs.sendTyping) return;
    if (typing === on) return;
    typing = on;
    void client.setTyping(roomId, on).catch(() => {});
  }

  function cancelTypingLocal() {
    clearTimeout(typingTimer);
    typingTimer = undefined;
    typing = false;
  }

  $effect(() => {
    if (!prefs.sendTyping) cancelTypingLocal();
  });

  function onInput() {
    resize();
    if (!prefs.sendTyping) return;
    if (!text.trim()) return setTyping(false);
    setTyping(true);
    clearTimeout(typingTimer);
    typingTimer = setTimeout(() => setTyping(false), 4000);
  }

  // Opens, moves or closes the suggestion popup for the `@` token ending at the caret.
  function syncMention() {
    const next = field ? activeMention(text, field.selectionStart) : null;
    if (next && (!mention || mentionKey(next) !== mentionKey(mention))) mentionIndex = 0;
    mention = next;
  }

  function onFieldInput() {
    syncMention();
    onInput();
  }

  function onFieldKeyup(e: KeyboardEvent) {
    if (e.key.startsWith('Arrow') || e.key === 'Home' || e.key === 'End') syncMention();
  }

  function dismissMention() {
    if (mention) mentionDismissed = mentionKey(mention);
  }

  // Replaces the `@query` token at the caret with the member's label and records whom it points at.
  async function pickMention(member: RoomMemberDto) {
    if (!field) return;
    const caret = field.selectionStart;
    const active = activeMention(text, caret);
    mention = null;
    if (!active) return;
    const label = memberLabel(member);
    const insert = `@${label} `;
    text = text.slice(0, active.start) + insert + text.slice(caret);
    mentionRecords = [...mentionRecords, { userId: member.user_id, label }];
    await tick();
    const next = active.start + insert.length;
    field?.setSelectionRange(next, next);
    onInput();
  }

  function resize() {
    if (!field) return;
    field.style.height = 'auto';
    field.style.height = `${Math.min(field.scrollHeight, 160)}px`;
  }

  async function submit() {
    if (!canSend) return;
    const body = text;
    const mentions = reconcileMentions(body, mentionRecords);
    text = '';
    mentionRecords = [];
    mention = null;
    clearTimeout(typingTimer);
    setTyping(false);
    await tickResize();
    if (threadRoot) await threadStore.send(body, mentions);
    else await timelineStore.send(body, mentions);
  }

  async function tickResize() {
    await Promise.resolve();
    resize();
  }

  function onKeydown(e: KeyboardEvent) {
    if (pickerOpen && !e.isComposing) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const step = e.key === 'ArrowDown' ? 1 : -1;
        mentionIndex = (highlighted + step + candidates.length) % candidates.length;
        return;
      }
      if ((e.key === 'Enter' && !e.shiftKey) || e.key === 'Tab') {
        const picked = candidates[highlighted];
        if (picked) {
          e.preventDefault();
          void pickMention(picked);
          return;
        }
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        dismissMention();
        return;
      }
    }
    // Enter sends on desktop by default. Phones always insert a newline and send with the Send button.
    // Elsewhere with enterToSend off, Enter inserts a newline and Ctrl/Cmd+Enter sends.
    const sendKey = viewport.isPhone
      ? e.ctrlKey || e.metaKey
      : prefs.enterToSend
        ? !e.shiftKey
        : e.ctrlKey || e.metaKey;
    if (e.key === 'Enter' && !e.isComposing && sendKey) {
      e.preventDefault();
      void submit();
    } else if (e.key === 'Escape') {
      if (emojiPicker.isOpen) emojiPicker.close();
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

  // Phones keep File and Poll behind the + button so the composer row stays uncluttered.
  function openAttachMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const file = { id: 'file', label: 'File', icon: Paperclip, onselect: () => void attach() };
    const poll = { id: 'poll', label: 'Poll', icon: ChartBar, onselect: () => (pollOpen = true) };
    contextMenu.open(canPickFiles() ? [file, poll] : [poll], { x: r.left, y: r.top });
  }

  // Right-click on a composer button offers exactly the action that button performs.
  function attachMenu(): MenuItem[] {
    return [{ id: 'attach', label: 'Attach file', icon: Paperclip, onselect: () => void attach() }];
  }

  let emojiButton: HTMLButtonElement | undefined = $state();

  function toggleEmoji() {
    if (emojiPicker.isOpen) {
      emojiPicker.close();
      return;
    }
    const anchor = emojiButton?.getBoundingClientRect();
    if (!anchor) return;
    emojiPicker.open({ anchor, onpick: addEmoji });
  }

  function emojiMenu(): MenuItem[] {
    return [
      {
        id: 'emoji',
        label: emojiPicker.isOpen ? 'Hide emoji picker' : 'Show emoji picker',
        icon: Smile,
        onselect: toggleEmoji,
      },
    ];
  }

  function sendMenu(): MenuItem[] {
    return [{ id: 'send', label: 'Send message', icon: Send, disabled: !canSend, onselect: () => void submit() }];
  }

  /** Insert at the caret (replacing a selection), then put the caret after the emoji. */
  function addEmoji(e: string) {
    const start = field?.selectionStart ?? text.length;
    const end = field?.selectionEnd ?? text.length;
    text = text.slice(0, start) + e + text.slice(end);
    onInput();
    void tick().then(() => {
      field?.focus();
      field?.setSelectionRange(start + e.length, start + e.length);
    });
  }

  function banner(): string {
    const it = replying ?? editing;
    if (!it) return '';
    const c = it.content;
    return c.kind === 'TextMessage' ? c.data.body : c.kind === 'Media' ? c.data.body || 'Attachment' : '';
  }
</script>

<div class="composer" class:attached={composing}>
  {#if composing}
    <div class="banner">
      <div class="banner-text">
        <span class="label">{editing ? 'Editing message' : `Replying to ${replying?.sender_display_name ?? replying?.sender}`}</span>
        <span class="preview">{banner()}</span>
      </div>
      <button
        type="button"
        class="round"
        aria-label={editing ? 'Cancel edit' : 'Cancel reply'}
        data-shortcut="Esc"
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

  <div class="field">
    {#if canPickFiles() && !threadRoot && !viewport.isPhone}
      <button type="button" class="round" aria-label="Attach file" title="Attach file" onclick={attach} use:contextmenu={attachMenu}>
        <Icon icon={Plus} size={20} />
      </button>
    {/if}
    {#if !threadRoot && viewport.isPhone}
      <button
        type="button"
        class="round"
        aria-label="Add attachment or poll"
        title="Add"
        aria-haspopup="menu"
        onclick={openAttachMenu}
      >
        <Icon icon={Plus} size={20} />
      </button>
    {/if}
    {#if !threadRoot && !viewport.isPhone}
      <button type="button" class="round" aria-label="Create poll" title="Create poll" onclick={() => (pollOpen = true)}>
        <Icon icon={ChartBar} size={20} />
      </button>
    {/if}
    {#if !threadRoot}
      <PollDialog open={pollOpen} onclose={() => (pollOpen = false)} />
    {/if}

    <textarea
      bind:this={field}
      bind:value={text}
      rows="1"
      data-composer-field
      {placeholder}
      aria-label={viewport.isPhone ? phoneName : hintSend ? 'Message. Press Ctrl+Enter or Cmd+Enter to send' : 'Message'}
      data-shortcut="Enter"
      role="combobox"
      aria-autocomplete="list"
      aria-expanded={pickerOpen}
      aria-controls={pickerOpen ? pickerId : undefined}
      aria-activedescendant={pickerOpen ? mentionOptionId(pickerId, highlighted) : undefined}
      oninput={onFieldInput}
      onkeydown={onKeydown}
      onkeyup={onFieldKeyup}
      onclick={syncMention}
      onblur={() => (mention = null)}
    ></textarea>

    <button
      bind:this={emojiButton}
      type="button"
      class="round"
      aria-label="Insert emoji"
      aria-expanded={emojiPicker.isOpen}
      title="Insert emoji"
      use:contextmenu={emojiMenu}
      onclick={toggleEmoji}
    >
      <Icon icon={Smile} size={20} />
    </button>
    {#if viewport.isPhone}
      <button
        type="button"
        class="send"
        aria-label="Send message"
        title="Send"
        use:contextmenu={sendMenu}
        disabled={!canSend}
        onpointerdown={(e) => e.preventDefault()}
        onclick={() => void submit()}
      >
        <Icon icon={Send} size={20} />
      </button>
    {/if}

    {#if pickerOpen}
      <MentionPicker
        id={pickerId}
        {candidates}
        {highlighted}
        onpick={(m) => void pickMention(m)}
        onhover={(i) => (mentionIndex = i)}
      />
    {/if}
  </div>
</div>

<style>
  .composer {
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter) var(--alloy-layout-gutter);
    background: var(--alloy-color-surface-canvas);
  }
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg) var(--alloy-radii-lg) 0 0;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
  }
  .banner-text {
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }
  .label {
    flex: none;
  }
  .preview {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .field {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    background: var(--alloy-color-surface-subtle);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
  }
  .field:focus-within {
    border-color: var(--alloy-color-accent-primary);
  }
  .attached .field {
    border-top-left-radius: 0;
    border-top-right-radius: 0;
  }
  textarea {
    flex: 1;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font: inherit;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-1);
    max-height: 160px;
    line-height: var(--alloy-typography-line-height-normal);
  }
  .round {
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
  .send {
    border: 0;
    border-radius: var(--alloy-radii-full);
    width: 44px;
    height: 44px;
    display: inline-grid;
    place-items: center;
    cursor: pointer;
    flex: none;
    background: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
  }
  .send:disabled {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-muted);
    cursor: default;
  }
  .banner .round {
    width: 28px;
    height: 28px;
  }
  @media (hover: hover) {
    .round:hover {
      background: var(--alloy-color-surface-hover);
      color: var(--alloy-color-text-primary);
    }
  }
  .round:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  @media (max-width: 899px) {
    .composer {
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-gutter)
        calc(var(--alloy-spacing-scale-2) + var(--alloy-safe-area-bottom));
    }
    .round,
    .banner .round {
      width: 44px;
      height: 44px;
    }
    textarea {
      box-sizing: border-box;
      flex: 1 1 0;
      width: 100%;
      min-width: 0;
      min-height: 44px;
      font-size: var(--alloy-typography-size-lg);
      padding: var(--alloy-spacing-scale-2) 0;
    }
  }
</style>
