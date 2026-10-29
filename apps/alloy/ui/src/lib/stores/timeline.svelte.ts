import { client, applyDiff } from '../ipc/index.js';
import type { EngineEvent, TimelineItemDto } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { errorMessage, expect } from './errors.js';
import { uploadsStore } from './uploads.svelte.js';
import { roomsStore } from './rooms.svelte.js';
import { prefs } from './prefs.svelte.js';
import { lastOwnEventId, readersByEvent } from '../content/receipts.js';
import { voiceExtension, writeTempFile } from '../platform/files.js';
import type { VoiceClip } from '../platform/voiceRecorder.js';

type TimelineDiffEvent = Extract<EngineEvent, { type: 'TimelineDiff' }>;

/** The open room's timeline plus message-level actions (reply/edit/react/redact/retry). */
class TimelineStore {
  roomId = $state<string | null>(null);
  items = $state<TimelineItemDto[]>([]);
  loading = $state(false);
  paginating = $state(false);
  reachedStart = $state(false);
  error = $state<string | null>(null);
  replyTo = $state<TimelineItemDto | null>(null);
  editing = $state<TimelineItemDto | null>(null);
  highlightedEventId = $state<string | null>(null);
  jumpTarget = $state<{ eventId: string; seq: number } | null>(null);
  /** Multi-select: event ids chosen while selection mode is on. */
  selecting = $state(false);
  selectedIds = $state<string[]>([]);
  /** Messages waiting for the "Delete for everyone?" confirmation. */
  pendingDelete = $state<TimelineItemDto[] | null>(null);
  /** A message waiting for the "Report this message?" confirmation. */
  pendingReport = $state<TimelineItemDto | null>(null);

  /** Who has read each of your messages, by event id (see `readersByEvent`). */
  readers = $derived(readersByEvent(this.items));
  /** Your newest sent message; only it shows the "seen by" avatars. */
  lastOwnEventId = $derived(lastOwnEventId(this.items));
  /** The selected messages in timeline order. */
  selectedItems = $derived(this.items.filter((i) => !!i.event_id && this.selectedIds.includes(i.event_id)));
  /** The newest event a read receipt was confirmed for; it is not published twice. */
  lastReceiptEvent = $state<string | null>(null);
  /** Why the last receipt could not be published, shown next to the timeline's other failures. */
  receiptError = $state<string | null>(null);

  private jumpSeq = 0;
  private highlightTimer: number | NodeJS.Timeout | null = null;
  private sub: Subscription<TimelineDiffEvent> | null = null;
  private batcher: FrameBatcher<TimelineDiffEvent> | null = null;
  private unlisten: (() => void) | null = null;
  /** The event whose receipt is in flight, so overlapping triggers share one request. */
  private receiptPending: string | null = null;

  /** Open a room (closing the previous one). Pass null to just close. */
  async open(roomId: string | null) {
    const closing = this.close();
    if (!roomId) return closing;
    void closing;
    this.roomId = roomId;
    this.loading = true;
    this.error = null;
    const batcher = new FrameBatcher<TimelineDiffEvent>((events) => {
      for (const e of events) if (e.payload.room_id === roomId) applyDiff(this.items, e.payload.diff);
      this.loading = false;
    });
    this.batcher = batcher;
    const sub = new Subscription<TimelineDiffEvent>((e) => e.payload.subscription_id, (e) => batcher.push(e));
    this.sub = sub;
    this.unlisten = client.subscribe((e) => {
      if (e.type === 'TimelineDiff' && e.payload.room_id === roomId) sub.push(e);
    });
    try {
      const res = expect(await client.subscribeTimeline(roomId), 'TimelineSubscribed');
      sub.attach(res.payload.subscription_id);
    } catch (e) {
      this.error = errorMessage(e);
      this.loading = false;
    }
  }

  async close() {
    // Reset all state synchronously: a concurrent `open()` for another room must never see (or
    // be clobbered by) the previous room's teardown, which awaits the network.
    this.unlisten?.();
    this.unlisten = null;
    const id = this.sub?.subscriptionId;
    this.sub?.close();
    this.sub = null;
    this.batcher?.clear();
    this.batcher = null;
    this.items.length = 0;
    this.roomId = null;
    this.reachedStart = false;
    this.replyTo = null;
    this.editing = null;
    this.lastReceiptEvent = null;
    this.receiptError = null;
    this.receiptPending = null;
    if (this.highlightTimer) {
      clearTimeout(this.highlightTimer);
      this.highlightTimer = null;
    }
    this.highlightedEventId = null;
    this.jumpTarget = null;
    this.selecting = false;
    this.selectedIds = [];
    this.pendingDelete = null;
    this.pendingReport = null;
    if (id) await client.unsubscribe(id).catch(() => {});
  }

  /** Load older history. */
  async loadOlder(count = 30) {
    const roomId = this.roomId;
    if (!roomId || this.paginating || this.reachedStart) return;
    this.paginating = true;
    try {
      const res = expect(await client.paginateBackwards(roomId, count), 'Paginated');
      // The user may have switched rooms meanwhile: another room's start says nothing about this one.
      if (this.roomId === roomId) this.reachedStart = res.payload.reached_start;
    } catch (e) {
      if (this.roomId === roomId) this.error = errorMessage(e);
    } finally {
      this.paginating = false;
    }
  }

  /** Send text (markdown by default); honors a pending reply or edit. */
  async send(body: string, mentions: string[] = [], mentionRoom = false) {
    if (!this.roomId) return;
    const roomId = this.roomId;
    const text = body.trim();
    if (!text) return;
    try {
      if (this.editing?.event_id) {
        const target = this.editing.event_id;
        this.editing = null;
        await client.editMessage(roomId, target, text, true);
      } else {
        const reply = this.replyTo?.event_id ?? null;
        this.replyTo = null;
        await client.sendText(roomId, text, {
          markdown: true,
          reply_to: reply,
          mentions: mentions.length ? mentions : null,
          mention_room: mentionRoom,
        });
      }
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** Sends a recorded clip as a voice message: the shell writes it to a temp file, the engine uploads it. */
  async sendVoice(clip: VoiceClip) {
    const roomId = this.roomId;
    if (!roomId) return;
    const txnId = crypto.randomUUID();
    uploadsStore.begin({ txnId, roomId, name: 'Voice message' });
    try {
      const path = await writeTempFile(clip.blob, voiceExtension(clip.mimeType));
      expect(
        await client.invoke({
          action: 'SendVoice',
          params: { room_id: roomId, file_path: path, duration_ms: clip.durationMs, waveform: clip.waveform, txn_id: txnId },
        }),
        'AttachmentSent',
      );
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      uploadsStore.end(txnId);
    }
  }

  async sendFile(path: string, mime?: string | null, body?: string | null) {
    if (!this.roomId) return;
    const roomId = this.roomId;
    const txnId = crypto.randomUUID();
    uploadsStore.begin({ txnId, roomId, name: path.split(/[\\/]/).pop() || path });
    try {
      await client.sendAttachment(roomId, path, { mime_type: mime ?? null, body: body ?? null, txn_id: txnId });
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      uploadsStore.end(txnId);
    }
  }

  async toggleReaction(item: TimelineItemDto, key: string) {
    if (!this.roomId || !item.event_id) return;
    await client.toggleReaction(this.roomId, item.event_id, key).catch((e) => {
      this.error = errorMessage(e);
    });
  }

  /** Returns null when the poll was sent, otherwise the message the dialog should show. */
  async sendPoll(question: string, answers: string[], disclosed: boolean, maxSelections: number): Promise<string | null> {
    if (!this.roomId) return 'No room is selected.';
    try {
      await client.sendPoll(this.roomId, question, answers, disclosed, maxSelections);
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  async respondToPoll(item: TimelineItemDto, answerIds: string[]) {
    if (!this.roomId || !item.event_id) return;
    await client.respondToPoll(this.roomId, item.event_id, answerIds).catch((e) => {
      this.error = errorMessage(e);
    });
  }

  async endPoll(item: TimelineItemDto) {
    if (!this.roomId || !item.event_id) return;
    await client.endPoll(this.roomId, item.event_id).catch((e) => {
      this.error = errorMessage(e);
    });
  }

  /** Redact several messages (Matrix deletes for everyone). A reason is only sent when given. */
  async redactItems(items: readonly TimelineItemDto[], reason: string | null = null) {
    const roomId = this.roomId;
    if (!roomId) return;
    for (const item of items) {
      if (!item.event_id) continue;
      await client.redactEvent(roomId, item.event_id, reason?.trim() || null);
    }
  }

  beginSelect(eventId: string) {
    this.selecting = true;
    this.selectedIds = [eventId];
  }

  toggleSelect(eventId: string) {
    this.selectedIds = this.selectedIds.includes(eventId)
      ? this.selectedIds.filter((id) => id !== eventId)
      : [...this.selectedIds, eventId];
    if (this.selectedIds.length === 0) this.selecting = false;
  }

  endSelect() {
    this.selecting = false;
    this.selectedIds = [];
  }

  /** Reports a message to the room's moderators and the homeserver. */
  async report(item: TimelineItemDto) {
    if (!this.roomId || !item.event_id) return;
    expect(await client.reportEvent(this.roomId, item.event_id), 'EventReported');
  }

  async redact(item: TimelineItemDto) {
    if (!this.roomId || !item.event_id) return;
    await client.redactEvent(this.roomId, item.event_id, null).catch((e) => {
      this.error = errorMessage(e);
    });
  }

  async retry(item: TimelineItemDto) {
    if (!this.roomId || !item.transaction_id) return;
    await client.retrySend(this.roomId, item.transaction_id).catch((e) => {
      this.error = errorMessage(e);
    });
  }

  async cancel(item: TimelineItemDto) {
    if (!this.roomId || !item.transaction_id) return;
    await client.cancelSend(this.roomId, item.transaction_id).catch((e) => {
      this.error = errorMessage(e);
    });
  }

  /**
   * Publish a read receipt for the newest confirmed event.
   *
   * The receipt is only remembered once the engine confirms it: a failed send is left retryable and
   * reported, instead of being swallowed while the event id was already marked as done.
   */
  async markRead() {
    const roomId = this.roomId;
    if (!roomId) return;
    // A hidden window has not read anything, whatever the scroll position says.
    if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;

    const eventId = this.newestEventId();
    if (!eventId) return;
    // "Show who has read your messages" governs publishing as well as display: with it off the room
    // still clears locally, but no read position leaves the device.
    if (!prefs.showReadReceipts) {
      roomsStore.markRead(roomId);
      return;
    }
    if (eventId === this.lastReceiptEvent || eventId === this.receiptPending) {
      roomsStore.markRead(roomId);
      return;
    }

    this.receiptPending = eventId;
    try {
      await client.markRead(roomId, eventId);
      this.lastReceiptEvent = eventId;
      this.receiptError = null;
      roomsStore.markRead(roomId);
    } catch (e) {
      // Keep the event retryable and say so: the unread badge is left alone rather than pretending.
      this.receiptError = errorMessage(e);
    } finally {
      if (this.receiptPending === eventId) this.receiptPending = null;
    }
  }

  /** The newest event that the engine has confirmed (a sending echo has no event id yet). */
  private newestEventId(): string | null {
    for (let i = this.items.length - 1; i >= 0; i--) {
      const id = this.items[i].event_id;
      if (id) return id;
    }
    return null;
  }

  startReply(item: TimelineItemDto) {
    this.editing = null;
    this.replyTo = item;
  }

  startEdit(item: TimelineItemDto) {
    this.replyTo = null;
    this.editing = item;
  }

  cancelCompose() {
    this.replyTo = null;
    this.editing = null;
  }

  /** Jump to an event in the timeline, scrolling to it and briefly highlighting it. */
  jumpTo(eventId: string) {
    if (this.highlightTimer) {
      clearTimeout(this.highlightTimer);
      this.highlightTimer = null;
    }
    this.highlightedEventId = eventId;
    this.jumpTarget = { eventId, seq: ++this.jumpSeq };
    this.highlightTimer = setTimeout(() => {
      if (this.highlightedEventId === eventId) {
        this.highlightedEventId = null;
      }
    }, 2000);
  }

  /**
   * Jump to an event, paging back through history until it is loaded. Resolves false when the
   * event cannot be reached: the room changed, history started, the page cap was hit, or a load failed.
   */
  async jumpToEvent(eventId: string, maxPages = 30): Promise<boolean> {
    const roomId = this.roomId;
    for (let page = 0; page <= maxPages; page++) {
      if (!roomId || this.roomId !== roomId) return false;
      if (this.hasEvent(eventId)) {
        this.jumpTo(eventId);
        return true;
      }
      if (this.reachedStart || this.paginating || page === maxPages) return false;
      const errorBefore = this.error;
      await this.loadOlder();
      if (this.error !== errorBefore) return false;
    }
    return false;
  }

  /** Check if an event is currently loaded in the timeline. */
  hasEvent(eventId: string | null | undefined): boolean {
    if (!eventId) return false;
    return this.items.some((item) => item.event_id === eventId);
  }
}

/**
 * Maps an event ID to its index in the timeline rows array.
 * Returns -1 if the event ID is not found or invalid.
 */
export function findRowIndexByEventId(
  rows: Array<{ event_id?: string | null }>,
  eventId: string | null | undefined
): number {
  if (!eventId) return -1;
  return rows.findIndex((r) => r.event_id === eventId);
}

export const timelineStore = new TimelineStore();
