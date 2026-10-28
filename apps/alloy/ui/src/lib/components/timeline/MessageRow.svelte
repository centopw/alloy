<script lang="ts">
  import type { TimelineItemDto } from '$lib/generated';
  import { Avatar, QuickReactions, UserName } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Reply, Pencil, Trash2, Copy, MessageSquare, Download, Check, CheckCheck, Users, Forward, Pin, PinOff, Flag } from '$lib/icons';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { threadStore } from '$lib/stores/thread.svelte';
  import { sanitizeHtml, SPOILER_SELECTOR } from '$lib/content/html';
  import { formatDay, formatTime, isEmojiOnly, isFileName } from '$lib/content/format';
  import { isHiddenState, stateEventSummary } from '$lib/content/stateEvents';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { emojiPicker } from '$lib/stores/emojiPicker.svelte';
  import { interceptLinks } from '$lib/platform/links';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { doubleTap } from '$lib/platform/doubleTap';
  import { swipe } from '$lib/platform/swipe';
  import { focusComposer } from '$lib/platform/typeToFocus';
  import { portal } from '$lib/platform/portal';
  import { displayNameFor } from '$lib/platform/names';
  import { joinNames } from '$lib/content/reactors';
  import { membersStore } from '$lib/stores/members.svelte';
  import { pinsStore } from '$lib/stores/pins.svelte';
  import { forwardStore } from '$lib/stores/forward.svelte';
  import { canDownloadMedia, downloadMedia } from '$lib/platform/saveFile';
  import { firstPreviewUrl } from '$lib/content/urls';
  import { errorMessage } from '$lib/stores/errors';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import { canCopyImage, copyImageFromUrl } from '$lib/platform/clipboard';
  import { fetchMediaFile } from '$lib/content/mediaFetch';
  import type { MenuItem, MenuOptions } from '$lib/stores/contextmenu.svelte';
  import MediaView from './MediaView.svelte';
  import AudioPlayer from './AudioPlayer.svelte';
  import AlbumGrid from './AlbumGrid.svelte';
  import LocationCard from './LocationCard.svelte';
  import PollView from './PollView.svelte';
  import ReplyQuote, { quote } from './ReplyQuote.svelte';
  import LinkPreview from './LinkPreview.svelte';
  import ReactionsSheet from './ReactionsSheet.svelte';

  interface Props {
    item: TimelineItemDto;
    prev: TimelineItemDto | null;
    /** Rendered inside the thread panel: room-level reply and thread actions are hidden. */
    inThread?: boolean;
    /** Every picture of the album this row leads; the row then renders them as one grid. */
    album?: readonly TimelineItemDto[] | null;
    /** The row is folded into the album of an earlier row and renders nothing. */
    albumHidden?: boolean;
  }
  let { item, prev, inThread = false, album = null, albumHidden = false }: Props = $props();

  // Same sender within this window continues the previous message as a compact row.
  const GROUP_MS = 7 * 60_000;
  // A removed placeholder hidden by prefs neither joins nor breaks a group.
  const isMessage = (i: TimelineItemDto | null) =>
    !!i &&
    ['TextMessage', 'Media', 'Poll', 'Location', 'Redacted', 'UnableToDecrypt'].includes(i.content.kind) &&
    !(i.content.kind === 'Redacted' && !prefs.showRemovedPlaceholder);

  const kind = $derived(item.content.kind);
  /** First plain URL in a text message; drives the link preview card. */
  const previewUrl = $derived(item.content.kind === 'TextMessage' ? firstPreviewUrl(item.content.data.body) : null);
  /** Rows the prefs hide entirely: removed placeholders and state events switched off. */
  const hidden = $derived(
    albumHidden ||
      (item.content.kind === 'Redacted' && !prefs.showRemovedPlaceholder) ||
      (item.content.kind === 'RoomState' && isHiddenState(item.content.data.state_type, prefs))
  );
  const grouped = $derived(
    isMessage(item) &&
      isMessage(prev) &&
      prev!.sender === item.sender &&
      item.timestamp_millis - prev!.timestamp_millis < GROUP_MS
  );
  const layout = $derived(prefs.messageLayout);
  /** Bubbles layout: the quote is a bubble stacked above the reply, not a panel inside it. */
  const stackedReply = $derived(layout === 'bubbles' && Boolean(item.reply_to));
  const time = $derived(formatTime(item.timestamp_millis, prefs.timeFormat));
  const avatarSize = $derived(layout === 'bubbles' ? 32 : viewport.isPhone ? 36 : 40);
  /** IRC always shows the name column; bubbles omit the head on your own messages. */
  const showHead = $derived(layout === 'irc' || (!grouped && !(layout === 'bubbles' && item.is_own)));
  /** A short run of emoji renders large when the pref is on. */  const jumbo = $derived(
    item.content.kind === 'TextMessage' && prefs.bigEmoji && isEmojiOnly(item.content.data.body)
  );
  /** A picture, video or sticker sent without a caption sits straight on the timeline, with no bubble behind it. */
  const bareMedia = $derived(
    !!album ||
    (item.content.kind === 'Media' &&
      ['image', 'video', 'sticker'].includes(item.content.data.media_kind ?? '') &&
      !item.reply_to &&
      !(item.content.data.body && !isFileName(item.content.data.body)))
  );
  const stateSummary = $derived(item.content.kind === 'RoomState' ? stateEventSummary(item.content.data) : null);
  const senderName = $derived(item.sender_display_name ?? item.sender);
  const failed = $derived(item.send_state.status === 'Failed');
  const sending = $derived(item.send_state.status === 'Sending');
  const canEdit = $derived(item.is_own && kind === 'TextMessage' && !!item.event_id);
  const canAct = $derived(!!item.event_id && (kind === 'TextMessage' || kind === 'Media' || kind === 'Location'));
  const canCopy = $derived(kind === 'TextMessage');
  const isImageMedia = $derived(
    item.content.kind === 'Media' &&
      (['image', 'sticker'].includes(item.content.data.media_kind ?? '') ||
        (item.content.data.mime_type && item.content.data.mime_type.startsWith('image/')))
  );
  const canCopyPicture = $derived(isImageMedia && canCopyImage());
  const canDownload = $derived(kind === 'Media' && canDownloadMedia());
  const highlighted = $derived(!!item.event_id && timelineStore.highlightedEventId === item.event_id);
  const roomDetails = $derived(membersStore.detailsOf(timelineStore.roomId));
  const canPin = $derived(canAct && (roomDetails?.can_pin ?? false));
  const pinned = $derived(pinsStore.isPinned(timelineStore.roomId, item.event_id));
  const canDelete = $derived(!!item.event_id && !sending && (item.is_own || (roomDetails?.can_redact_others ?? false)));
  const selected = $derived(!!item.event_id && timelineStore.selectedIds.includes(item.event_id));

  let rowEl = $state<HTMLElement | undefined>();

  /** Opens the shared emoji picker beside `anchor`; choosing an emoji toggles that reaction. */
  function openReactions(anchor: DOMRect) {
    emojiPicker.open({ anchor, onpick: (emoji) => void timelineStore.toggleReaction(item, emoji) });
  }

  /** Emoji this user already reacted with on the message; shown pressed in the quick bar. */
  const myReactions = $derived((item.reactions ?? []).filter((r) => r.by_me).map((r) => r.key));

  function reactWith(emoji: string) {
    void timelineStore.toggleReaction(item, emoji);
  }

  /** Display name of a room member for tooltips and the who-reacted list. */
  function nameOf(userId: string): string {
    const m = membersStore.membersOf(timelineStore.roomId).find((x) => x.user_id === userId);
    return displayNameFor(userId, null, m?.display_name ?? null);
  }

  /** Other people who have read this message of yours (their receipt is on it or on a later event). */
  const readers = $derived(item.is_own && item.event_id ? (timelineStore.readers.get(item.event_id) ?? []) : []);
  const showTick = $derived(prefs.showReadReceipts && item.is_own && !!item.event_id && !sending && !failed);
  const readByOthers = $derived(readers.length > 0);
  const readerNames = $derived(readers.map(nameOf));
  /** Stacked avatars of readers, only under your newest message and only in small rooms. */
  const seenAvatars = $derived(
    showTick &&
      readByOthers &&
      item.event_id === timelineStore.lastOwnEventId &&
      (membersStore.detailsOf(timelineStore.roomId)?.joined_members ?? membersStore.membersOf(timelineStore.roomId).length) <= 20
  );

  // Who reacted: the sheet lists every reactor per emoji.
  let whoOpen = $state(false);
  let whoKey = $state<string | null>(null);
  function openWho(key: string | null) {
    whoKey = key;
    whoOpen = true;
  }

  // Phones: swipe right on a message to reply to it; double-tap to react with the first quick reaction.
  let swipeX = $state(0);
  let swiping = $state(false);
  const swipeOptions = $derived({
    direction: 'right' as const,
    threshold: 64,
    enabled: viewport.isPhone && canAct && !inThread && !timelineStore.selecting,
    edgeGuard: 24,
    onmove: (offset: number) => {
      swiping = true;
      swipeX = offset;
    },
    onend: () => {
      swiping = false;
      swipeX = 0;
    },
    oncommit: () => {
      timelineStore.startReply(item);
      focusComposer();
    },
  });

  let burst = $state<{ x: number; y: number; emoji: string; id: number } | null>(null);
  let burstSeq = 0;
  function onDoubleTap(point: { x: number; y: number }) {
    const emoji = prefs.quickReactions[0];
    if (!viewport.isPhone || !canAct || !emoji) return;
    const removing = myReactions.includes(emoji);
    reactWith(emoji);
    if (removing || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const id = ++burstSeq;
    burst = { ...point, emoji, id };
    window.setTimeout(() => {
      if (burst?.id === id) burst = null;
    }, 650);
  }

  /** A click on a hidden spoiler reveals it; every other click follows the link rules. */
  function onBodyClick(e: MouseEvent) {
    const spoiler = e.target instanceof Element ? e.target.closest<HTMLElement>(SPOILER_SELECTOR) : null;
    if (spoiler && !spoiler.hasAttribute('data-revealed')) {
      e.preventDefault();
      spoiler.setAttribute('data-revealed', '');
      return;
    }
    interceptLinks(e);
  }

  /** Reaction bar and mobile quick actions of the message menu. */
  function menuOptions(): MenuOptions | undefined {
    if (!canAct) return undefined;
    const bubble = rowEl?.querySelector<HTMLElement>('.bubble, .media, .body') ?? rowEl;
    const bubbleRect = bubble?.getBoundingClientRect();
    const rowRect = rowEl?.getBoundingClientRect();
    const anchor = bubbleRect
      ? {
          x: bubbleRect.left,
          y: bubbleRect.top,
          width: bubbleRect.width,
          height: bubbleRect.height,
          align: item.is_own ? ('right' as const) : ('left' as const),
        }
      : undefined;
    const spotlight = rowRect
      ? {
          x: rowRect.left,
          y: rowRect.top,
          width: rowRect.width,
          height: rowRect.height,
        }
      : undefined;

    const phoneBar: MenuItem[] = [];
    if (canAct && !inThread) {
      phoneBar.push({ id: 'reply', label: 'Reply', icon: Reply, onselect: () => timelineStore.startReply(item) });
    }

    if (canCopy) {
      phoneBar.push({ id: 'copy', label: 'Copy', icon: Copy, onselect: () => void navigator.clipboard.writeText(quote(item)) });
    } else if (canCopyPicture) {
      phoneBar.push({ id: 'copy-picture', label: 'Copy', icon: Copy, onselect: () => void copyPictureItem() });
    } else if (canPin && item.event_id && timelineStore.roomId) {
      const roomId = timelineStore.roomId;
      const eventId = item.event_id;
      phoneBar.push(
        pinned
          ? { id: 'unpin', label: 'Unpin', icon: PinOff, onselect: () => void pinsStore.setPinned(roomId, eventId, false) }
          : { id: 'pin', label: 'Pin', icon: Pin, onselect: () => void pinsStore.setPinned(roomId, eventId, true) },
      );
    } else if (canDownload) {
      phoneBar.push({ id: 'download', label: 'Download', icon: Download, onselect: downloadMediaItem });
    }

    if (canAct && !inThread) {
      phoneBar.push({
        id: 'forward',
        label: 'Forward',
        icon: Forward,
        onselect: () => item.event_id && timelineStore.roomId && forwardStore.begin(timelineStore.roomId, [item.event_id]),
      });
    }

    return {
      reactions: {
        emojis: prefs.quickReactions,
        active: myReactions,
        onpick: reactWith,
        onmore: () => rowEl && openReactions(rowEl.getBoundingClientRect()),
        anchor,
      },
      anchor,
      spotlight,
      phoneBar,
    };
  }

  function openThread() {
    const roomId = timelineStore.roomId;
    if (roomId && item.event_id) void threadStore.open(roomId, item.event_id);
  }

  /** Opens the sender's profile card at the pointer, or centred on the avatar when activated from the keyboard. */
  function openSenderProfile(e: MouseEvent) {
    let at = { x: e.clientX, y: e.clientY };
    if (e.detail === 0) {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      at = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }
    profileCard.open(item.sender, timelineStore.roomId, at);
  }

  /** In selection mode a click anywhere on the message toggles it instead of acting on what was clicked. */
  function onRowClickCapture(e: MouseEvent) {
    if (!timelineStore.selecting || !canAct || !item.event_id) return;
    e.preventDefault();
    e.stopPropagation();
    timelineStore.toggleSelect(item.event_id);
  }

  const html = $derived(
    item.content.kind === 'TextMessage' && item.content.data.formatted_body
      ? sanitizeHtml(item.content.data.formatted_body)
      : null
  );

  /** Saves a media message's file through the native save dialog; failures show in the timeline toast. */
  function downloadMediaItem() {
    if (item.content.kind !== 'Media') return;
    const { uri, mime_type, body } = item.content.data;
    downloadMedia(uri, mime_type, body || 'media').catch((err: unknown) => {
      timelineStore.error = errorMessage(err);
    });
  }

  async function copyPictureItem() {
    if (item.content.kind !== 'Media') return;
    try {
      const { uri, mime_type } = item.content.data;
      const url = await fetchMediaFile(uri, mime_type);
      await copyImageFromUrl(url);
    } catch (err: unknown) {
      timelineStore.error = errorMessage(err);
    }
  }

  /** Context-menu entries for this message. Groups that do not apply are omitted, not disabled. */
  function menuItems(): MenuItem[] {
    const actions: MenuItem[] = [];

    // 1. Reply
    if (canAct && !inThread) {
      actions.push({ id: 'reply', label: 'Reply', icon: Reply, onselect: () => timelineStore.startReply(item) });
      actions.push({ id: 'thread', label: 'Reply in thread', icon: MessageSquare, onselect: openThread });
    }

    // 2. Edit (own text)
    if (canEdit) {
      actions.push({ id: 'edit', label: 'Edit', icon: Pencil, onselect: () => timelineStore.startEdit(item) });
    }

    // 3. Copy text / Copy picture
    if (canCopy) {
      actions.push({ id: 'copy-text', label: 'Copy text', icon: Copy, onselect: () => void navigator.clipboard.writeText(quote(item)) });
    }
    if (canCopyPicture) {
      actions.push({ id: 'copy-picture', label: 'Copy picture', icon: Copy, onselect: () => void copyPictureItem() });
    }
    actions.push({ id: 'copy-sender', label: 'Copy sender ID', icon: Copy, onselect: () => void navigator.clipboard.writeText(item.sender) });

    // 4. Pin/Unpin
    if (canPin && item.event_id && timelineStore.roomId) {
      const roomId = timelineStore.roomId;
      const eventId = item.event_id;
      actions.push(
        pinned
          ? { id: 'unpin', label: 'Unpin', icon: PinOff, onselect: () => void pinsStore.setPinned(roomId, eventId, false) }
          : { id: 'pin', label: 'Pin', icon: Pin, onselect: () => void pinsStore.setPinned(roomId, eventId, true) },
      );
    }

    // 5. Download (media)
    if (canDownload) {
      actions.push({ id: 'download', label: 'Download', icon: Download, onselect: downloadMediaItem });
    }

    // 6. Forward
    if (canAct && !inThread) {
      actions.push({
        id: 'forward',
        label: 'Forward',
        icon: Forward,
        onselect: () => item.event_id && timelineStore.roomId && forwardStore.begin(timelineStore.roomId, [item.event_id]),
      });
    }

    // 7. Select
    if (canAct && item.event_id) {
      const eventId = item.event_id;
      actions.push({ id: 'select', label: 'Select', icon: Check, onselect: () => timelineStore.beginSelect(eventId) });
    }

    // 8. Destructive (Delete, Report)
    const destructive: MenuItem[] = [];
    if (canDelete) {
      destructive.push({ id: 'delete', label: 'Delete', icon: Trash2, danger: true, onselect: () => (timelineStore.pendingDelete = [item]) });
    }
    if (!item.is_own && canAct) {
      destructive.push({ id: 'report', label: 'Report', icon: Flag, danger: true, onselect: () => (timelineStore.pendingReport = item) });
    }

    if (destructive.length > 0) {
      if (actions.length > 0) actions.push({ separator: true });
      actions.push(...destructive);
    }

    return actions;
  }
</script>

{#snippet tick()}
  {#if showTick}
    <span
      class="tick"
      class:read={readByOthers}
      role="img"
      aria-label={readByOthers ? `Seen by ${joinNames(readerNames)}` : 'Sent'}
      data-tooltip={readByOthers ? `Seen by ${joinNames(readerNames)}` : 'Sent'}
    >
      <Icon icon={readByOthers ? CheckCheck : Check} size={16} />
    </span>
  {/if}
{/snippet}

{#if !hidden}
  {#if item.content.kind === 'DayDivider'}
  <div class="divider"><span>{formatDay(item.content.data.timestamp_millis)}</span></div>
{:else if item.content.kind === 'RoomState'}
  <div class="system">
    {stateSummary ?? (item.content.data.state_type === 'membership' ? 'Membership changed' : 'Room updated')}
  </div>
{:else if item.content.kind === 'Virtual'}
  <div class="spacer"></div>
{:else}
  <div
    class="row"
    class:grouped
    class:own={item.is_own}
    class:replied={stackedReply}
    class:jumbo
    class:bare={bareMedia}
    class:highlighted
    class:swiping
    class:selecting={timelineStore.selecting && canAct}
    class:selected
    onclickcapture={onRowClickCapture}
    class:mention={item.mentions_me && !item.is_own}
    style:transform={swipeX > 0 ? `translateX(${swipeX}px)` : undefined}
    data-layout={layout}
    data-event-id={item.event_id ?? undefined}
    bind:this={rowEl}
    use:contextmenu={{ items: menuItems, options: menuOptions }}
    use:swipe={swipeOptions}
    use:doubleTap={onDoubleTap}
  >
    {#if swipeX > 0}
      <span class="swipe-hint" class:ready={swipeX >= 64} style:opacity={Math.min(1, swipeX / 64)} aria-hidden="true">
        <Icon icon={Reply} size={24} />
      </span>
    {/if}
    {#if timelineStore.selecting && canAct}
      <button type="button" class="select-mark" aria-label={selected ? 'Deselect message' : 'Select message'} aria-pressed={selected}>
        <span class="select-dot"><Icon icon={Check} size={16} /></span>
      </button>
    {/if}
    <div class="gutter">
      {#if layout === 'irc' || (layout === 'modern' && grouped)}
        <span class="gutter-time">{time}{@render tick()}</span>
      {:else if layout === 'modern' || (!grouped && !item.is_own)}
        <button type="button" class="avatar-btn" aria-label="Show profile of {senderName}" onclick={openSenderProfile}>
          <Avatar
            userId={item.sender}
            displayName={senderName}
            src={item.sender_avatar_url ?? null}
            size={avatarSize}
          />
        </button>
      {/if}
    </div>

    <div class="stack">
      {#if showHead}
        <div class="head">
          <UserName userId={item.sender} displayName={item.sender_display_name} roomId={timelineStore.roomId} interactive />
          {#if layout !== 'bubbles'}<span class="ts">{time}</span>{@render tick()}{/if}
        </div>
      {/if}

      <div class="message-stack">
        {#if stackedReply && item.reply_to}
          <ReplyQuote replyTo={item.reply_to} stacked own={item.is_own} />
        {/if}

        <div class="bubble" class:has-reactions={(item.reactions ?? []).length > 0}>
          {#if item.reply_to && !stackedReply}
            <div class="reply-slot">
              <ReplyQuote replyTo={item.reply_to} inOwnBubble={layout === 'bubbles' && item.is_own} />
            </div>
          {/if}

          {#if item.content.kind === 'TextMessage'}
            {#if html}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <div class="body html" onclick={onBodyClick} onauxclick={interceptLinks}>{@html html}{#if item.edited}<span class="edited">(edited)</span>{/if}</div>
            {:else}
              <div class="body">{item.content.data.body}{#if item.edited}<span class="edited">(edited)</span>{/if}</div>
            {/if}
            {#if previewUrl && timelineStore.roomId}
              <LinkPreview url={previewUrl} roomId={timelineStore.roomId} />
            {/if}
          {:else if item.content.kind === 'Media'}
            <div class="block">
              {#if album}
                <AlbumGrid items={album} />
              {:else if item.content.data.media_kind === 'audio'}
                <AudioPlayer
                  uri={item.content.data.uri}
                  mimeType={item.content.data.mime_type}
                  body={item.content.data.body ?? ''}
                  sizeBytes={item.content.data.size_bytes}
                  durationMs={item.content.data.duration_ms ?? null}
                  waveform={item.content.data.waveform ?? null}
                  voice={item.content.data.voice ?? false}
                />
              {:else}
                <MediaView
                  eventId={item.event_id}
                  uri={item.content.data.uri}
                  mimeType={item.content.data.mime_type}
                  body={item.content.data.body ?? ''}
                  mediaKind={item.content.data.media_kind ?? 'file'}
                  sizeBytes={item.content.data.size_bytes}
                  width={item.content.data.width}
                  height={item.content.data.height}
                  thumbnailUri={item.content.data.thumbnail_uri}
                />
              {/if}
              {#if item.content.data.media_kind === 'image' && item.content.data.body && !isFileName(item.content.data.body)}
                <div class="body caption">{item.content.data.body}{#if item.edited}<span class="edited">(edited)</span>{/if}</div>
              {/if}
            </div>
          {:else if item.content.kind === 'Redacted'}
            <div class="body muted">Message deleted</div>
          {:else if item.content.kind === 'UnableToDecrypt'}
            <div class="body muted">Waiting for this message. Verify this device to read it.</div>
          {:else if item.content.kind === 'Poll'}
            <div class="block"><PollView {item} poll={item.content.data} /></div>
          {:else if item.content.kind === 'Location'}
            <div class="block">
              <LocationCard
                geoUri={item.content.data.geo_uri}
                description={item.content.data.description}
                live={item.content.data.live ?? false}
                liveUntil={item.content.data.live_until_millis ?? null}
              />
            </div>
          {:else}
            <div class="body muted">This message type is not supported yet.</div>
          {/if}

          {#if item.is_own && sending}
            <div class="state sending" role="status" aria-label="Sending">Sending</div>
          {:else if failed}
            <div class="failed-actions">
              <span class="err">Failed to send</span>
              <button type="button" onclick={() => timelineStore.retry(item)}>Retry</button>
              <button type="button" onclick={() => timelineStore.cancel(item)}>Discard</button>
            </div>
          {/if}

          {#if item.thread_summary && !inThread}
            <button type="button" class="thread-chip" onclick={openThread} aria-label="Open thread, {item.thread_summary.reply_count} {item.thread_summary.reply_count === 1 ? 'reply' : 'replies'}">
              <Icon icon={MessageSquare} size={16} />
              <span>{item.thread_summary.reply_count} {item.thread_summary.reply_count === 1 ? 'reply' : 'replies'}</span>
            </button>
          {/if}
          {#if (item.reactions ?? []).length > 0}
            <div class="reactions">
              {#each item.reactions ?? [] as r (r.key)}
                <button
                  type="button"
                  class="chip"
                  class:mine={r.by_me}
                  onclick={() => void timelineStore.toggleReaction(item, r.key)}
                  aria-pressed={r.by_me}
                  aria-label="{r.key} {r.count}"
                  data-tooltip={joinNames(r.senders.map(nameOf))}
                  use:contextmenu={() => [{ id: 'who', label: 'See who reacted', icon: Users, onselect: () => openWho(r.key) }]}
                >
                  <span class="pill"><span>{r.key}</span><span class="count">{r.count}</span></span>
                </button>
              {/each}
            </div>
          {/if}

          <span class="bubble-time">{time}{@render tick()}</span>
        </div>
      </div>
      {#if seenAvatars}
        <div class="seen" data-tooltip="Seen by {joinNames(readerNames)}">
          {#each readers.slice(0, 3) as userId (userId)}
            {@const m = membersStore.membersOf(timelineStore.roomId).find((x) => x.user_id === userId)}
            <Avatar {userId} displayName={m?.display_name ?? null} src={m?.avatar_url ?? null} size={16} />
          {/each}
          {#if readers.length > 3}<span class="seen-more">+{readers.length - 3}</span>{/if}
        </div>
      {/if}
    </div>

    {#if canAct}
      <div class="toolbar" role="toolbar" aria-label="Message actions">
        <QuickReactions
          emojis={prefs.quickReactions}
          active={myReactions}
          onpick={reactWith}
          onmore={() => rowEl && openReactions(rowEl.getBoundingClientRect())}
        />
        <span class="tool-sep" aria-hidden="true"></span>
        {#if !inThread}
          <button type="button" aria-label="Reply" title="Reply" onclick={() => timelineStore.startReply(item)}>
            <Icon icon={Reply} size={16} />
          </button>
        {/if}
        {#if item.event_id && !inThread}
          <button type="button" aria-label="Reply in thread" title="Reply in thread" onclick={openThread}>
            <Icon icon={MessageSquare} size={16} />
          </button>
        {/if}
        {#if canEdit}
          <button type="button" aria-label="Edit" title="Edit" onclick={() => timelineStore.startEdit(item)}>
            <Icon icon={Pencil} size={16} />
          </button>
        {/if}
        {#if canDelete}
          <button type="button" aria-label="Delete" title="Delete" onclick={() => (timelineStore.pendingDelete = [item])}>
            <Icon icon={Trash2} size={16} />
          </button>
        {/if}
      </div>

    {/if}
  </div>
  {#if whoOpen}
    <ReactionsSheet {item} open={whoOpen} initialKey={whoKey} onclose={() => (whoOpen = false)} />
  {/if}
  {#if burst}
    <span class="burst" style:left="{burst.x}px" style:top="{burst.y}px" aria-hidden="true" use:portal>{burst.emoji}</span>
  {/if}
{/if}
{/if}

<style>
  .row {
    position: relative;
    display: flex;
    align-items: flex-start;
    margin: 0 var(--alloy-layout-row-inset);
    padding: var(--alloy-spacing-scale-1) var(--alloy-layout-row-pad-x);
    border-radius: var(--alloy-radii-md);
    min-width: 0;
  }
  .row:not(.grouped) {
    padding-top: var(--alloy-spacing-scale-2);
  }
  .row:focus-within {
    background: var(--alloy-color-surface-hover);
  }
  @media (hover: hover) {
    .row:hover {
      background: var(--alloy-color-surface-hover);
    }
    /* Bubbles are the surfaces: the row itself stays transparent (the delete/selection tints still show). */
    .row[data-layout='bubbles']:hover:not(.selected):not(.highlighted) {
      background: transparent;
    }
  }
  .row[data-layout='bubbles']:focus-within:not(.selected):not(.highlighted) {
    background: transparent;
  }
  .row.highlighted {
    background: var(--alloy-color-accent-subtle);
  }
  /* A message that mentions you carries an accent edge. */
  .row.mention {
    box-shadow: inset 3px 0 0 var(--alloy-color-accent-primary);
  }
  /* Multi-select: a round check sits at the left edge and a tap anywhere on the message toggles it. */
  .row.selecting {
    cursor: pointer;
    padding-left: calc(var(--alloy-layout-row-pad-x) + 44px);
  }
  .row.selected {
    background: var(--alloy-color-accent-subtle);
  }
  .select-mark {
    position: absolute;
    top: 0;
    left: 0;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: pointer;
  }
  .select-dot {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 2px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-full);
    color: transparent;
  }
  .row.selected .select-dot {
    border-color: var(--alloy-color-accent-primary);
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
  }
  /* Phone swipe-to-reply: the row follows the finger and springs back on release. */
  .row {
    transition: transform var(--alloy-motion-duration-normal) ease-out;
  }
  .row.swiping {
    transition: none;
  }
  .swipe-hint {
    position: absolute;
    top: 50%;
    left: calc(-1 * var(--alloy-spacing-scale-8));
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-muted);
    translate: 0 -50%;
    pointer-events: none;
  }
  .swipe-hint.ready {
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
  }
  .tick {
    display: inline-flex;
    margin-left: var(--alloy-spacing-scale-1);
    vertical-align: middle;
    color: var(--alloy-color-text-muted);
  }
  .tick.read {
    color: var(--alloy-color-accent-primary);
  }
  .seen {
    display: flex;
    align-items: center;
    align-self: flex-end;
    margin-top: 2px;
  }
  .seen > :global(* + *) {
    margin-left: -6px;
  }
  .seen > :global(*) {
    border-radius: var(--alloy-radii-full);
    box-shadow: 0 0 0 2px var(--alloy-color-surface-canvas);
  }
  .seen-more {
    margin-left: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .burst {
    position: fixed;
    z-index: 120;
    font-size: 56px;
    line-height: 1;
    pointer-events: none;
    transform: translate(-50%, -50%);
    animation: alloy-burst 600ms ease-out forwards;
  }
  @keyframes alloy-burst {
    0% {
      opacity: 0;
      scale: 0.3;
    }
    30% {
      opacity: 1;
      scale: 1.25;
    }
    100% {
      opacity: 0;
      scale: 1;
      translate: 0 -28px;
    }
  }
  .gutter {
    flex: none;
    width: 56px;
    display: flex;
    justify-content: center;
  }
  .gutter-time {
    padding-top: 4px;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    opacity: 0;
  }
  .row:hover .gutter-time,
  .row:focus-within .gutter-time {
    opacity: 1;
  }
  .stack {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }
  .ts {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .body {
    min-width: 0;
    color: var(--alloy-color-text-primary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    line-height: var(--alloy-typography-line-height-normal);
  }
  .body.html {
    white-space: normal;
  }
  .body.html :global(p) {
    margin: 0;
  }
  .body.html :global(p + p) {
    margin-top: 0.5em;
  }
  .body.html :global(pre) {
    margin: var(--alloy-spacing-scale-1) 0;
    padding: var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
    overflow-x: auto;
  }
  .body.html :global(pre) :global(code) {
    padding: 0;
    background: none;
    border-radius: 0;
  }
  .body.html :global(code) {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: 0.92em;
    padding: 0 var(--alloy-spacing-scale-1);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-sm);
  }
  .body.html :global(a) {
    color: var(--alloy-color-accent-primary);
    text-decoration: underline;
  }
  .body.html :global(span[data-mx-spoiler]:not([data-revealed])) {
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-active);
    filter: blur(6px);
    cursor: pointer;
    user-select: none;
  }
  .body.html :global(span[data-mx-spoiler]:not([data-revealed])) :global(a) {
    pointer-events: none;
  }
  .body.html :global(blockquote) {
    margin: var(--alloy-spacing-scale-1) 0;
    padding-left: var(--alloy-spacing-scale-3);
    border-left: 3px solid var(--alloy-color-border-prominent);
    color: var(--alloy-color-text-secondary);
  }
  .caption {
    margin-top: var(--alloy-spacing-scale-1);
  }
  .muted {
    font-style: italic;
    color: var(--alloy-color-text-muted);
  }
  .edited {
    margin-left: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .state {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  /* A pending echo has no byte progress to show, so the label breathes to prove the row is alive. */
  .state.sending {
    animation: sending-pulse 1.6s ease-in-out infinite;
  }
  @keyframes sending-pulse {
    0%,
    100% {
      opacity: 0.5;
    }
    50% {
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .state.sending {
      animation: none;
      opacity: 0.85;
    }
  }
  .failed-actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    font-size: var(--alloy-typography-size-sm);
  }
  .err {
    color: var(--alloy-color-status-error);
  }
  .failed-actions button {
    background: none;
    border: 0;
    color: var(--alloy-color-status-error);
    cursor: pointer;
    padding: 2px 0;
    font: inherit;
    text-decoration: underline;
  }
  .thread-chip {
    display: inline-flex;
    gap: var(--alloy-spacing-scale-1);
    align-items: center;
    margin-top: var(--alloy-spacing-scale-1);
    padding: 2px var(--alloy-spacing-scale-2);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  @media (hover: hover) {
    .thread-chip:hover {
      background: var(--alloy-color-surface-hover);
    }
  }
  .thread-chip:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .reactions {
    /* Height of one chip's tap area; the pill inside is smaller on phones. */
    --chip-box: 24px;
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-1);
    margin-top: var(--alloy-spacing-scale-1);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    min-height: var(--chip-box);
    padding: 0;
    border: 0;
    background: none;
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .pill {
    display: inline-flex;
    gap: var(--alloy-spacing-scale-1);
    align-items: center;
    padding: 2px var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
    border: 1px solid var(--alloy-color-border-subtle);
    /* Opaque: a reaction is read at a glance, and it floats half over the bubble, so it cannot
       borrow the surface behind it. */
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    line-height: var(--alloy-typography-line-height-normal);
  }
  @media (hover: hover) {
    .chip:hover .pill {
      background: var(--alloy-color-surface-hover);
    }
  }
  .chip.mine .pill {
    border-color: var(--alloy-color-accent-primary);
    background: var(--alloy-color-accent-subtle);
  }
  /* On the accent bubble the chip keeps its own solid plate; only the edge picks up the accent so
     it still belongs to the message. */
  .row.own[data-layout='bubbles'] .pill {
    border-color: color-mix(
      in srgb,
      var(--alloy-color-accent-primary) 45%,
      var(--alloy-color-border-subtle)
    );
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
  }
  .count {
    color: var(--alloy-color-text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .toolbar {
    position: absolute;
    top: calc(-1 * var(--alloy-spacing-scale-3));
    right: var(--alloy-spacing-scale-4);
    display: none;
    align-items: center;
    gap: 2px;
    padding: 3px;
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-2xl);
    box-shadow: var(--alloy-glass-shadow-soft), var(--alloy-glass-highlight);
    z-index: 2;
  }
  @media (min-width: 900px) and (hover: hover) {
    .row:hover .toolbar,
    .row:focus-within .toolbar {
      display: flex;
    }
  }
  .tool-sep {
    width: 1px;
    height: 16px;
    margin: 0 2px;
    background: var(--alloy-color-border-subtle);
  }
  .toolbar > button {
    display: inline-grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .toolbar > button:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .toolbar button:focus-visible,
  .failed-actions button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .chip:focus-visible {
    outline: none;
  }
  .chip:focus-visible .pill {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .avatar-btn {
    display: inline-grid;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: none;
    cursor: pointer;
  }
  .avatar-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .divider {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-4);
  }
  /* A date is a rule across the conversation, not another floating chip. */
  .divider::before,
  .divider::after {
    content: '';
    flex: 1;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--alloy-chat-rim));
  }
  .divider::after {
    background: linear-gradient(90deg, var(--alloy-chat-rim), transparent);
  }
  .divider span {
    padding: 0;
    border: 0;
    background: none;
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .system {
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-4);
    text-align: center;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-muted);
  }
  .spacer {
    height: 1px;
  }

  /* Message layouts. Bubbles is the default (ADR 0013); modern and IRC stay available in prefs. */
  .bubble {
    display: contents;
  }
  .bubble-time {
    display: none;
  }
  .block {
    min-width: 0;
  }
  .row.jumbo .body {
    font-size: 2.5em;
    line-height: 1.2;
  }

  .row[data-layout='irc'] {
    --irc-name-width: 9rem;
  }
  .row[data-layout='irc'] .gutter {
    width: 4.5rem;
  }
  .row[data-layout='irc'] .gutter-time {
    padding-top: 0;
    opacity: 1;
  }
  .row[data-layout='irc'] .stack {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: baseline;
  }
  .row[data-layout='irc'] .head {
    flex: 0 0 var(--irc-name-width);
    min-width: 0;
    overflow: hidden;
    padding-right: var(--alloy-spacing-scale-2);
  }
  .row[data-layout='irc'] .ts {
    display: none;
  }
  .row[data-layout='irc'] .body {
    flex: 1 1 0;
    min-width: 0;
  }
  .row[data-layout='irc'] .reply-slot,
  .row[data-layout='irc'] .block,
  .row[data-layout='irc'] .state,
  .row[data-layout='irc'] .failed-actions,
  .row[data-layout='irc'] .thread-chip,
  .row[data-layout='irc'] .reactions {
    flex: 0 0 calc(100% - var(--irc-name-width));
    margin-left: var(--irc-name-width);
    min-width: 0;
  }

  .row[data-layout='bubbles'] {
    padding-top: 2px;
    padding-bottom: 2px;
  }
  /* A new sender group starts a new breath; messages inside one group stay tight (ADR 0013). */
  .row[data-layout='bubbles']:not(.grouped) {
    padding-top: var(--alloy-spacing-scale-3);
  }
  .row[data-layout='bubbles'] .gutter {
    width: 40px;
  }
  .row.own[data-layout='bubbles'] .gutter {
    display: none;
  }
  .row[data-layout='bubbles'] .stack {
    align-items: flex-start;
  }
  .row.own[data-layout='bubbles'] .stack {
    align-items: flex-end;
  }
  /* Quote and bubble are one stack: the wrapper is invisible by default, and a replied message
     gives them a single shared column so both edges line up at a matched pair of tight corners. */
  .row[data-layout='bubbles'] .message-stack {
    display: contents;
  }
  .row.replied[data-layout='bubbles'] .message-stack {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    width: fit-content;
    max-width: var(--alloy-bubble-max-width);
    min-width: 0;
  }
  .row.replied[data-layout='bubbles'] .message-stack .bubble {
    max-width: 100%;
  }
  .row[data-layout='bubbles'] .head {
    margin-bottom: 2px;
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-tight);
  }
  .row[data-layout='bubbles'] .bubble {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: var(--alloy-bubble-max-width);
    padding: var(--alloy-spacing-bubble-y) var(--alloy-spacing-bubble-x);
    border: 1px solid color-mix(in srgb, var(--alloy-color-text-primary) 8%, transparent);
    border-radius: var(--alloy-bubble-radius);
    background: var(--alloy-bubble-other-surface);
    color: var(--alloy-color-bubble-other-text);
  }
  /* One flat saturated surface: no gradient, no top highlight, no coloured halo. The rim is the
     only edge treatment, so the bubble reads as ink on the canvas rather than as a glass bead. */
  .row.own[data-layout='bubbles'] .bubble {
    border-color: color-mix(in srgb, var(--alloy-color-bubble-own-text) 12%, transparent);
    background: var(--alloy-color-bubble-own-background);
    color: var(--alloy-color-bubble-own-text);
    box-shadow: none;
  }
  .row[data-layout='bubbles'] .bubble .body,
  .row[data-layout='bubbles'] .bubble .edited,
  .row[data-layout='bubbles'] .bubble .muted,
  .row[data-layout='bubbles'] .bubble .state,
  .row.own[data-layout='bubbles'] .thread-chip {
    color: inherit;
  }
  .row[data-layout='bubbles'] .bubble .edited,
  .row[data-layout='bubbles'] .bubble .muted {
    opacity: 0.75;
  }
  .row.own[data-layout='bubbles'] .body.html :global(a) {
    color: inherit;
  }
  /* Reactions sit on the bottom edge of the bubble, half over it, as in Messenger. */
  .row[data-layout='bubbles'] .bubble.has-reactions {
    margin-bottom: var(--alloy-spacing-scale-3);
  }
  .row[data-layout='bubbles'] .reactions {
    position: absolute;
    left: var(--alloy-spacing-scale-2);
    bottom: calc(var(--chip-box) / -2);
    flex-wrap: nowrap;
    margin: 0;
    z-index: 1;
  }
  .row.own[data-layout='bubbles'] .reactions {
    left: auto;
    right: var(--alloy-spacing-scale-2);
  }
  .row.grouped[data-layout='bubbles']:not(.own) .bubble {
    border-top-left-radius: var(--alloy-bubble-radius-tail);
  }
  .row.grouped.own[data-layout='bubbles'] .bubble {
    border-top-right-radius: var(--alloy-bubble-radius-tail);
  }
  /* The reply bubble continues the quote card above it: the junction is square on both sides, so
     the pair reads as one stacked message with a header rather than as two rounded cards. */
  .row.replied[data-layout='bubbles'] .bubble {
    border-top-left-radius: 0;
    border-top-right-radius: 0;
  }
  /* The time is always present: showing it only on hover grew the bubble and shifted every message below. */
  .row[data-layout='bubbles'] .bubble-time {
    display: block;
    text-align: right;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .row.own[data-layout='bubbles'] .bubble-time {
    color: inherit;
    opacity: 0.7;
  }
  .row.jumbo[data-layout='bubbles'] .bubble,
  .row.bare[data-layout='bubbles'] .bubble {
    padding: 0;
    border: 0;
    background: none;
  }
  .row.highlighted[data-layout='bubbles'] .bubble {
    box-shadow: 0 0 0 2px var(--alloy-color-accent-primary);
  }

  /* Phone: full-width rows and 44px touch targets. */
  @media (max-width: 899px) {
    .row {
      margin: 0;
      padding-left: var(--alloy-spacing-scale-3);
      padding-right: var(--alloy-spacing-scale-3);
      border-radius: 0;
    }
    .gutter {
      width: 48px;
    }
    .avatar-btn {
      position: relative;
    }
    .avatar-btn::after {
      content: '';
      position: absolute;
      inset: -8px;
    }
    .thread-chip {
      min-height: 44px;
      padding: 0 var(--alloy-spacing-scale-3);
    }
    .reactions {
      --chip-box: 44px;
    }
    .chip {
      min-height: 44px;
      padding: 0 var(--alloy-spacing-scale-1);
    }
    .row[data-layout='bubbles'] .bubble.has-reactions {
      margin-bottom: var(--alloy-spacing-scale-6);
    }
    .failed-actions button {
      display: inline-flex;
      align-items: center;
      min-height: 44px;
      padding: 0 var(--alloy-spacing-scale-2);
    }
    .block {
      max-width: 100%;
    }
    .row[data-layout='irc'] {
      --irc-name-width: 6.5rem;
    }
    .row[data-layout='irc'] .gutter {
      width: 3.5rem;
    }
  }

  /* Touch devices have no hover: gutter times stay visible. */
  @media (hover: none) {
    /* A long press opens the message menu (which has Copy text), not the system selection handles. */
    .row {
      -webkit-user-select: none;
      user-select: none;
      -webkit-touch-callout: none;
    }
    .row .gutter-time {
      opacity: 1;
    }
  }
</style>
