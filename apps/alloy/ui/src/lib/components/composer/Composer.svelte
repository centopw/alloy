<script lang="ts">
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { draftsStore } from '$lib/stores/drafts.svelte';
  import { threadStore } from '$lib/stores/thread.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { client } from '$lib/ipc';
  import { prefs } from '$lib/stores/prefs.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChartBar, Ellipsis, File, Image, ImagePlus, MapPin, Navigation, Plus, Send, Smile, Type, X } from '$lib/icons';
  import {
    canPickFiles,
    importPastedImagePath,
    isTauriRuntime,
    looksLikeFilePath,
    pastedImageFiles,
    pastedImageName,
    pickFiles,
    pickMediaFiles,
    savePastedImage
  } from '$lib/platform/files';
  import { nativeClipboardHasImage, nativeClipboardImage } from '$lib/platform/clipboardImage';
  import { stagePictureFromPath } from './stageHelper';
  import { errorMessage } from '$lib/stores/errors';
  import { isVoiceMessagesSupported } from '$lib/platform/capabilities';
  import { canLocate } from '$lib/platform/geolocation';
  import { liveLocation } from '$lib/stores/liveLocation.svelte';
  import LocationDialog from './LocationDialog.svelte';
  import VoiceRecorder from './VoiceRecorder.svelte';
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
    roomMentionOption,
    containsRoomMention,
    ROOM_MENTION_ID,
    type ActiveMention,
    type MentionRecord
  } from '$lib/stores/mentions';
  import MentionPicker from './MentionPicker.svelte';
  import PollDialog from './PollDialog.svelte';
  import FormatToolbar from './FormatToolbar.svelte';
  import { wrapKindForKey, wrapSelection, type WrapKind } from '$lib/content/markdownWrap';
  import { Button, Modal, TextField } from '../common';

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
  let locationOpen = $state(false);
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
  /** The microphone stands in for Send while there is nothing to send (not while editing or in a thread). */
  let recording = $state(false);
  const canRecord = $derived(!threadRoot && !editing && !canSend && isTauriRuntime() && isVoiceMessagesSupported());

  // Formatting: a floating toolbar over a selection on desktop, an "Aa" row on phones, plus shortcuts.
  let selected = $state(false);
  let formatOpen = $state(false);
  let linkOpen = $state(false);
  let linkUrl = $state('');
  let linkRange = { start: 0, end: 0 };
  const showFormat = $derived(!threadRoot && !recording && (viewport.isPhone ? formatOpen : selected));

  function syncSelection() {
    selected = !!field && field.selectionEnd > field.selectionStart;
  }

  async function applyWrap(kind: WrapKind, url = '', range?: { start: number; end: number }) {
    if (!field) return;
    const r = wrapSelection(text, range?.start ?? field.selectionStart, range?.end ?? field.selectionEnd, kind, url);
    text = r.text;
    await tick();
    field.focus();
    field.setSelectionRange(r.start, r.end);
    syncSelection();
    onInput();
  }

  function onWrap(kind: WrapKind) {
    if (kind !== 'link') return void applyWrap(kind);
    linkRange = { start: field?.selectionStart ?? 0, end: field?.selectionEnd ?? 0 };
    linkUrl = 'https://';
    linkOpen = true;
  }

  function confirmLink() {
    const url = linkUrl.trim();
    linkOpen = false;
    if (url && url !== 'https://') void applyWrap('link', url, linkRange);
    else field?.focus();
  }
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
  const canMentionRoom = $derived(membersStore.detailsOf(roomId)?.can_mention_room ?? false);
  const candidates = $derived(
    mention
      ? [roomMentionOption(mention.query, canMentionRoom), ...rankMembers(membersStore.membersOf(roomId), mention.query, MENTION_LIMIT)]
          .filter((m): m is RoomMemberDto => m !== null)
          .slice(0, MENTION_LIMIT)
      : []
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

  // Whether this room's saved draft has been read; typing is only saved after that, so an empty
  // field never overwrites a draft that has not loaded yet.
  let draftReady = $state(false);

  async function restoreDraft(id: string) {
    const draft = await draftsStore.load(id);
    if (roomId !== id) return;
    if (draft && !draft.editOf && text === '') {
      text = draft.text;
      void tickResize();
    }
    draftReady = true;
  }

  // Switching rooms starts with that room's draft (or a clean composer) and loads its members for mentions.
  // Leaving a room saves its draft at once.
  $effect(() => {
    const id = roomId;
    const inThread = !!threadRoot;
    text = '';
    draftReady = false;
    untrack(() => emojiPicker.close());
    mention = null;
    mentionDismissed = null;
    mentionRecords = [];
    // Without loaded members the picker stays empty; sending is unaffected.
    // Untracked: the cache read inside must not make members arriving later wipe the draft.
    untrack(() => void membersStore.loadMembers(roomId).catch(() => {}));
    if (!inThread) untrack(() => void restoreDraft(id));
    return () => {
      if (inThread || !untrack(() => draftReady) || untrack(() => editing)) return;
      draftsStore.flushPending(id, untrack(() => text), { replyTo: untrack(() => replying?.event_id ?? null) });
    };
  });

  // The text of an unsent message is kept per room, shortly after typing stops.
  $effect(() => {
    const current = text;
    const id = roomId;
    if (threadRoot || editing || !draftReady) return;
    untrack(() => draftsStore.save(id, current, { replyTo: replying?.event_id ?? null }));
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
    if (member.user_id === ROOM_MENTION_ID) {
      const insertRoom = '@room ';
      text = text.slice(0, active.start) + insertRoom + text.slice(caret);
      await tick();
      field?.setSelectionRange(active.start + insertRoom.length, active.start + insertRoom.length);
      onInput();
      return;
    }
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
    const mentionRoom = canMentionRoom && containsRoomMention(body);
    text = '';
    mentionRecords = [];
    mention = null;
    clearTimeout(typingTimer);
    setTyping(false);
    await tickResize();
    if (threadRoot) await threadStore.send(body, mentions, mentionRoom);
    else await timelineStore.send(body, mentions, mentionRoom);
  }

  async function tickResize() {
    await Promise.resolve();
    resize();
  }

  // Pictures pasted into the field wait here until the user sends them. A path found on the clipboard is only a
  // suggestion of a file, so nothing leaves the device without an explicit Send.
  interface StagedPicture {
    path: string;
    name: string;
    preview: string | null;
  }
  let staged = $state<StagedPicture[]>([]);

  function dropPreviews(items: StagedPicture[]) {
    for (const item of items) if (item.preview) URL.revokeObjectURL(item.preview);
  }

  function unstage(path: string) {
    dropPreviews(staged.filter((item) => item.path === path));
    staged = staged.filter((item) => item.path !== path);
  }

  async function sendStaged() {
    const items = staged;
    staged = [];
    for (const item of items) await timelineStore.sendFile(item.path);
    dropPreviews(items);
  }

  $effect(() => () => dropPreviews(staged));
  async function stageFromClipboard() {
    const rawPath = nativeClipboardImage();
    if (!rawPath) return;
    try {
      const picture = await stagePictureFromPath(rawPath);
      if (picture) staged.push(picture);
    } catch (err) {
      timelineStore.error = errorMessage(err);
    }
  }


  async function onPaste(e: ClipboardEvent) {
    if (!canPickFiles() || threadRoot) return;
    const pictures = pastedImageFiles(e.clipboardData);
    if (pictures.length > 0) {
      e.preventDefault();
      for (const file of pictures) {
        try {
          staged.push({ path: await savePastedImage(file), name: pastedImageName(file), preview: URL.createObjectURL(file) });
        } catch (err) {
          timelineStore.error = errorMessage(err);
        }
      }
      return;
    }
    const pasted = e.clipboardData?.getData('text/plain') ?? '';
    if (pasted === '' || (nativeClipboardHasImage() && pasted.trim() === '')) {
      if (nativeClipboardHasImage()) {
        e.preventDefault();
        await stageFromClipboard();
      }
      return;
    }
    if (!looksLikeFilePath(pasted)) return;
    // The decision to attach needs the shell, which answers after the paste event is over: take the text out
    // of the event now and put it back if it turns out not to be a picture.
    e.preventDefault();
    let path: string | null = null;
    try {
      path = await importPastedImagePath(pasted);
    } catch (err) {
      timelineStore.error = errorMessage(err);
    }
    if (path) {
      staged.push({ path, name: path.split(/[\\/]/).pop() ?? path, preview: null });
    } else {
      field?.focus();
      document.execCommand('insertText', false, pasted);
    }
  }
  function onKeydown(e: KeyboardEvent) {
    // With a pasted picture waiting and nothing typed, Enter sends the picture.
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing && staged.length > 0 && text.trim() === '') {
      e.preventDefault();
      void sendStaged();
      return;
    }
    // Ctrl/Cmd shortcuts for formatting the selection.
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.isComposing && !threadRoot) {
      const kind = wrapKindForKey(e.key, e.shiftKey);
      if (kind) {
        e.preventDefault();
        onWrap(kind);
        return;
      }
    }
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
    // Up arrow in an empty field edits your newest message (Telegram, Slack).
    if (e.key === 'ArrowUp' && !e.isComposing && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey && !threadRoot && !composing && text === '') {
      const last = timelineStore.items.findLast((i) => i.is_own && i.content.kind === 'TextMessage' && !!i.event_id);
      if (last) {
        e.preventDefault();
        timelineStore.startEdit(last);
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

  /** Picks one or more files and sends them in the order chosen; pictures sent together form an album. */
  async function attach() {
    for (const path of await pickFiles()) await timelineStore.sendFile(path);
  }

  /**
   * Picks pictures or videos and sends them in the order chosen. On Android and iOS this opens the
   * system media chooser, so the user lands in the gallery instead of a file browser.
   */
  async function attachMedia() {
    for (const path of await pickMediaFiles()) await timelineStore.sendFile(path);
  }

  // Location choices: send where you are now, or share it live for a while.
  function locationItems(): MenuItem[] {
    const live: MenuItem = {
      id: 'live-location',
      label: 'Share live location',
      icon: Navigation,
      submenu: {
        items: [
          { id: 'live-15', label: 'For 15 minutes', onselect: () => void liveLocation.start(roomId, 15 * 60_000) },
          { id: 'live-60', label: 'For 1 hour', onselect: () => void liveLocation.start(roomId, 60 * 60_000) },
          { id: 'live-480', label: 'For 8 hours', onselect: () => void liveLocation.start(roomId, 8 * 60 * 60_000) },
        ],
      },
    };
    return [{ id: 'send-location', label: 'Send my location', icon: MapPin, onselect: () => (locationOpen = true) }, live];
  }

  // The attach menu leads with media: a picture or a video is the main thing to send, a plain file is
  // second, and the poll and the location stay reachable one level down under More (ADR 0007 tiers
  // menus instead of growing flat lists).
  function attachItems(): MenuItem[] {
    const items: MenuItem[] = [];
    if (canPickFiles()) {
      items.push({ id: 'media', label: 'Photo or video', icon: ImagePlus, onselect: () => void attachMedia() });
      items.push({ id: 'file', label: 'File', icon: File, onselect: () => void attach() });
    }
    if (nativeClipboardHasImage()) {
      items.push({
        id: 'clipboard-image',
        label: 'Picture from clipboard',
        icon: Image,
        onselect: () => void stageFromClipboard(),
      });
    }
    items.push(moreItem());
    return items;
  }

  /** Poll and location: the secondary ways to send something, behind one More row. */
  function moreItem(): MenuItem {
    return {
      id: 'more',
      label: 'More',
      icon: Ellipsis,
      submenu: {
        items: [
          { id: 'poll', label: 'Poll', icon: ChartBar, onselect: () => (pollOpen = true) },
          ...(canLocate() ? locationItems() : []),
        ],
      },
    };
  }

  function openAttachMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.open(attachItems(), { x: r.left, y: r.top }, {
      anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
      ariaLabel: 'Add attachment',
    });
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
  {#if showFormat}
    <div class="format" class:floating={!viewport.isPhone}>
      <FormatToolbar onwrap={onWrap} />
    </div>
  {/if}
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

  {#if staged.length > 0}
    <div class="staged" role="group" aria-label="Pictures ready to send">
      {#each staged as item (item.path)}
        <div class="picture">
          {#if item.preview}
            <img src={item.preview} alt="" />
          {/if}
          <span class="picture-name">{item.name}</span>
          <button type="button" class="round" aria-label="Remove {item.name}" data-tooltip="Remove" onclick={() => unstage(item.path)}>
            <Icon icon={X} size={16} />
          </button>
        </div>
      {/each}
      <Button size="sm" onclick={() => void sendStaged()}>{staged.length === 1 ? 'Send picture' : `Send ${staged.length} pictures`}</Button>
    </div>
  {/if}

  <div class="field" class:recording>
    {#if !threadRoot}
      <button
        type="button"
        class="round"
        aria-label="Add attachment, poll or location"
        data-tooltip="Add"
        aria-haspopup="menu"
        onclick={openAttachMenu}
        use:contextmenu={attachItems}
      >
        <Icon icon={Plus} size={20} />
      </button>
      <PollDialog open={pollOpen} onclose={() => (pollOpen = false)} />
      <LocationDialog open={locationOpen} {roomId} onclose={() => (locationOpen = false)} />
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
      onpaste={onPaste}
      onkeyup={onFieldKeyup}
      onclick={syncMention}
      onselect={syncSelection}
      onpointerup={syncSelection}
      onblur={() => {
        mention = null;
        selected = false;
      }}
    ></textarea>

    {#if viewport.isPhone && !threadRoot}
      <button
        type="button"
        class="round"
        aria-label="Text formatting"
        aria-pressed={formatOpen}
        data-tooltip="Formatting"
        onpointerdown={(e) => e.preventDefault()}
        onclick={() => (formatOpen = !formatOpen)}
      >
        <Icon icon={Type} size={20} />
      </button>
    {/if}
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
    {#if viewport.isPhone && (canSend || !canRecord)}
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
    {#if canRecord || recording}
      <div class="voice-slot" class:recording>
        <VoiceRecorder bind:recording onsend={(clip) => timelineStore.sendVoice(clip)} onerror={(m) => (timelineStore.error = m)} />
      </div>
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

<Modal open={linkOpen} title="Add link" onclose={() => (linkOpen = false)} width="420px">
  <form
    class="link-form"
    onsubmit={(e) => {
      e.preventDefault();
      confirmLink();
    }}
  >
    <TextField bind:value={linkUrl} type="url" aria-label="Link address" placeholder="https://" autocomplete="off" />
    <div class="link-actions">
      <Button variant="ghost" type="button" onclick={() => (linkOpen = false)}>Cancel</Button>
      <Button type="submit">Add link</Button>
    </div>
  </form>
</Modal>

<style>
  .composer {
    position: relative;
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter) var(--alloy-layout-gutter);
    background: var(--alloy-color-surface-canvas);
  }
  .format.floating {
    position: absolute;
    right: var(--alloy-layout-gutter);
    bottom: calc(100% - var(--alloy-spacing-scale-2));
    z-index: 5;
  }
  .format:not(.floating) {
    margin-bottom: var(--alloy-spacing-scale-2);
  }
  .link-form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
  }
  .link-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  .staged {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    margin-bottom: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
  }
  .picture {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .picture img {
    width: 40px;
    height: 40px;
    object-fit: cover;
    border-radius: var(--alloy-radii-md);
  }
  .picture-name {
    min-width: 0;
    max-width: 16rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  /* While a voice message is recorded its bar takes over the whole field. */
  .voice-slot {
    display: contents;
  }
  .voice-slot.recording {
    display: flex;
    flex: 1;
    align-items: center;
    min-width: 0;
  }
  .field.recording > :not(.voice-slot) {
    display: none;
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
