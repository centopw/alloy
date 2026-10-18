<script lang="ts">
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { threadStore } from '$lib/stores/thread.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { client } from '$lib/ipc';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChartBar, Plus, Smile, X } from '$lib/icons';
  import { canPickFiles, pickFile } from '$lib/platform/files';
  import { EMOJI_PICKER } from '$lib/content/emoji';
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
  let emojiOpen = $state(false);
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
  const placeholder = $derived(
    roomName ? `Message ${roomName.length > 28 ? `${roomName.slice(0, 27)}…` : roomName}` : 'Message'
  );
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
    emojiOpen = false;
    mention = null;
    mentionDismissed = null;
    mentionRecords = [];
    // Without loaded members the picker stays empty; sending is unaffected.
    // Untracked: the cache read inside must not make members arriving later wipe the draft.
    untrack(() => void membersStore.loadMembers(roomId).catch(() => {}));
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

  <div class="field">
    {#if canPickFiles() && !threadRoot}
      <button type="button" class="round" aria-label="Attach file" title="Attach file" onclick={attach}>
        <Icon icon={Plus} size={20} />
      </button>
    {/if}
    {#if !threadRoot}
      <button type="button" class="round" aria-label="Create poll" title="Create poll" onclick={() => (pollOpen = true)}>
        <Icon icon={ChartBar} size={20} />
      </button>
      <PollDialog open={pollOpen} onclose={() => (pollOpen = false)} />
    {/if}

    <textarea
      bind:this={field}
      bind:value={text}
      rows="1"
      {placeholder}
      aria-label="Message"
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

    <div class="emoji-wrap">
      <button
        type="button"
        class="round"
        aria-label="Insert emoji"
        aria-expanded={emojiOpen}
        title="Insert emoji"
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
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-4);
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
  .banner .round {
    width: 28px;
    height: 28px;
  }
  .round:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .round:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .emoji-wrap {
    position: relative;
  }
  .emoji-grid {
    position: absolute;
    bottom: calc(100% + var(--alloy-spacing-scale-2));
    right: 0;
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 2px;
    padding: var(--alloy-spacing-scale-2);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-elevation-md);
    z-index: 5;
  }
  .emoji-grid button {
    border: 0;
    background: transparent;
    font-size: 1.25rem;
    padding: var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-sm);
    cursor: pointer;
  }
  .emoji-grid button:hover {
    background: var(--alloy-color-surface-hover);
  }
</style>
