import { client, applyDiff } from '../ipc/index.js';
import type { CapabilitiesDto, EngineEvent, TimelineItemDto, UserId } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { errorMessage, expect } from './errors.js';
import { uploadsStore } from './uploads.svelte.js';
import { roomsStore } from './rooms.svelte.js';
import { voiceExtension, writeTempFile } from '../platform/files.js';
import type { VoiceClip } from '../platform/voiceRecorder.js';

type TimelineDiffEvent = Extract<EngineEvent, { type: 'TimelineDiff' }>;

/** Rooms kept for instant reopening; the least recently used is evicted with `CloseTimeline`. */
export const TIMELINE_CACHE_LIMIT = 8;

interface CachedTimeline {
  items: TimelineItemDto[];
  reachedStart: boolean;
  scrollTop: number;
}

/** Delivery status of one row, narrowed from the protocol union. */
export type SendStatus = TimelineItemDto['send_state']['status'];

/** True while the message has not reached the network yet. */
export function isPending(item: TimelineItemDto): boolean {
  const status = item.send_state.status;
  return status === 'Queued' || status === 'Sending';
}

/**
 * Whether the edit action applies: your own sent text, an account that allows editing, and inside
 * the network's edit window when it has one.
 */
export function canEditItem(
  item: TimelineItemDto,
  capabilities: CapabilitiesDto | null,
  now = Date.now()
): boolean {
  if (!capabilities?.edit || !item.is_own || !item.event_id) return false;
  if (item.content.kind !== 'TextMessage' || item.send_state.status !== 'Sent') return false;
  const windowSecs = capabilities.edit_window_secs;
  if (windowSecs === null) return true;
  return now - item.timestamp_millis <= windowSecs * 1000;
}

/** Whether "delete for everyone" applies to the row. */
export function canDeleteForEveryone(
  item: TimelineItemDto,
  capabilities: CapabilitiesDto | null
): boolean {
  return !!item.event_id && item.send_state.status === 'Sent' && !!capabilities?.delete_for_everyone;
}

/**
 * Who has read each of your messages: the engine reports, per message, the users that have read it,
 * so a user who read a later message also read this one. Your own id is left out.
 */
export function readersByEvent(items: readonly TimelineItemDto[]): Map<EventIdKey, UserId[]> {
  const me = items.find((i) => i.is_own)?.sender;
  const result = new Map<EventIdKey, UserId[]>();
  const seen = new Set<UserId>();
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    for (const user of item.read_by) if (user !== me) seen.add(user);
    if (item.is_own && item.event_id) result.set(item.event_id, [...seen]);
  }
  return result;
}

type EventIdKey = string;

/** Event id of your newest sent message, or null. */
export function lastOwnEventId(items: readonly TimelineItemDto[]): string | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.is_own && item.event_id) return item.event_id;
  }
  return null;
}

/**
 * A `Reset` replaces the window the engine sends, not necessarily everything the cache holds: the
 * cache can carry older history the reset does not cover. Keep cached items older than the reset
 * window, take the reset as authoritative for what it contains, and keep local echoes it has not
 * seen yet. Deduplicated by internal id, so reopening never doubles a row.
 */
export function mergeTimelineReset(
  cached: readonly TimelineItemDto[],
  reset: readonly TimelineItemDto[]
): TimelineItemDto[] {
  // A day divider carries no event id either, so the kind is what tells it apart from an unsent
  // message. Dividers belong to the window that generated them and the reset brings its own; an
  // echo has to keep its place at the end.
  const isVirtual = (i: TimelineItemDto) => i.content.kind === 'DayDivider';
  const resetIds = new Set(reset.map((i) => i.event_id).filter((id): id is string => !!id));
  // A reset normally opens with the day divider for its window, and virtual items carry no real
  // timestamp. Measuring the window from that divider would make every cached message look newer
  // than it and drop the history the cache exists to keep, so the window starts at the first event.
  const firstResetTs = reset.find((i) => i.event_id)?.timestamp_millis ?? Number.POSITIVE_INFINITY;
  const older = cached.filter(
    (i) => i.event_id && !resetIds.has(i.event_id) && i.timestamp_millis < firstResetTs
  );
  const echoes = cached.filter((i) => !i.event_id && !isVirtual(i));
  const seen = new Set<string>();
  const out: TimelineItemDto[] = [];
  for (const item of [...older, ...reset, ...echoes]) {
    if (seen.has(item.internal_id)) continue;
    seen.add(item.internal_id);
    out.push(item);
  }
  return out;
}

/** The open room's timeline plus message-level actions (reply/edit/react/delete/retry). */
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

  /** Who has read each of your messages, by event id. */
  readers = $derived(readersByEvent(this.items));
  /** Your newest sent message; only it shows the "seen by" avatars. */
  lastOwnEventId = $derived(lastOwnEventId(this.items));
  /** The selected messages in timeline order. */
  selectedItems = $derived(this.items.filter((i) => !!i.event_id && this.selectedIds.includes(i.event_id)));
  /** The newest event a read position was confirmed for; it is not published twice. */
  lastReceiptEvent = $state<string | null>(null);
  /** Why the last read position could not be published, shown next to the timeline's other failures. */
  receiptError = $state<string | null>(null);
  /** What the open chat's account allows; null until the account list has loaded. */
  capabilities = $derived(roomsStore.activeCapabilities);

  private jumpSeq = 0;
  private highlightTimer: number | NodeJS.Timeout | null = null;
  private sub: Subscription<TimelineDiffEvent> | null = null;
  private batcher: FrameBatcher<TimelineDiffEvent> | null = null;
  private unlisten: (() => void) | null = null;
  /** The event whose receipt is in flight, so overlapping triggers share one request. */
  private receiptPending: string | null = null;
  /** Where the last open room was scrolled to; plain state, it changes on every scroll frame. */
  private scrollTop = 0;
  /** Scroll offset to apply once the reopened room's rows are rendered. */
  pendingScroll = $state<number | null>(null);
  /** Cached timelines by room, least recently used first. */
  private cache = new Map<string, CachedTimeline>();
  /** The next diff for the open room is its first `Reset`, which merges rather than clears. */
  private awaitingReset = false;

  /** Open a room (closing the previous one). Pass null to just close. */
  async open(roomId: string | null) {
    const closing = this.close();
    if (!roomId) return closing;
    void closing;
    this.roomId = roomId;
    this.error = null;
    const cached = this.cache.get(roomId);
    if (cached) {
      // Touch the entry so it is the last to be evicted, and start from what was on screen.
      this.cache.delete(roomId);
      this.cache.set(roomId, cached);
      this.items = cached.items.slice();
      this.reachedStart = cached.reachedStart;
      this.scrollTop = cached.scrollTop;
      this.pendingScroll = cached.scrollTop;
      this.loading = false;
    } else {
      this.items = [];
      this.reachedStart = false;
      this.scrollTop = 0;
      this.pendingScroll = null;
      this.loading = true;
    }
    this.awaitingReset = true;
    const batcher = new FrameBatcher<TimelineDiffEvent>((events) => {
      for (const e of events) {
        if (e.payload.room_id !== roomId) continue;
        const diff = e.payload.diff;
        if (diff.type === 'Reset') {
          this.items = this.awaitingReset
            ? mergeTimelineReset(this.items, diff.content.values)
            : diff.content.values;
          this.awaitingReset = false;
        } else {
          applyDiff(this.items, diff);
        }
      }
      this.loading = false;
    });
    this.batcher = batcher;
    const sub = new Subscription<TimelineDiffEvent>((e) => e.payload.subscription_id, (e) => batcher.push(e));
    this.sub = sub;
    this.unlisten = client.subscribe((e) => {
      if (e.type === 'TimelineDiff' && e.payload.room_id === roomId) sub.push(e);
    });
    try {
      const res = expect(
        await client.invoke({ action: 'SubscribeTimeline', params: { room_id: roomId } }),
        'TimelineSubscribed'
      );
      sub.attach(res.payload.subscription_id);
    } catch (e) {
      this.error = errorMessage(e);
      this.loading = false;
    }
  }

  async close() {
    // Reset all state synchronously: a concurrent `open()` for another room must never see (or
    // be clobbered by) the previous room's teardown, which awaits the network.
    if (this.roomId && this.items.length > 0) this.remember(this.roomId, this.scrollTop);
    this.unlisten?.();
    this.unlisten = null;
    const id = this.sub?.subscriptionId;
    this.sub?.close();
    this.sub = null;
    this.batcher?.clear();
    this.batcher = null;
    this.items = [];
    this.roomId = null;
    this.awaitingReset = false;
    this.scrollTop = 0;
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
    if (id) await client.invoke({ action: 'Unsubscribe', params: { subscription_id: id } }).catch(() => {});
  }

  /** The timeline reports its scroll offset, so reopening the room can put the reader back. */
  rememberScroll(offset: number) {
    this.scrollTop = offset;
  }

  /** Drops every cached room (sign-out); their timelines are closed in the engine. */
  clearCache() {
    for (const roomId of this.cache.keys()) {
      void client.invoke({ action: 'CloseTimeline', params: { room_id: roomId } }).catch(() => {});
    }
    this.cache.clear();
    this.pendingScroll = null;
    this.scrollTop = 0;
  }

  /** Keeps a room's timeline for reopening and evicts the least recently used past the limit. */
  private remember(roomId: string, scrollTop: number) {
    this.cache.delete(roomId);
    this.cache.set(roomId, { items: this.items.slice(), reachedStart: this.reachedStart, scrollTop });
    while (this.cache.size > TIMELINE_CACHE_LIMIT) {
      const oldest = this.cache.keys().next().value as string;
      this.cache.delete(oldest);
      // Best effort: the engine drops that room's timeline; opening it again rebuilds from the store.
      void client.invoke({ action: 'CloseTimeline', params: { room_id: oldest } }).catch(() => {});
    }
  }

  /** Load older history. */
  async loadOlder(count = 30) {
    const roomId = this.roomId;
    if (!roomId || this.paginating || this.reachedStart) return;
    this.paginating = true;
    try {
      const res = expect(
        await client.invoke({ action: 'PaginateBackwards', params: { room_id: roomId, count } }),
        'Paginated'
      );
      // The user may have switched rooms meanwhile: another room's start says nothing about this one.
      if (this.roomId === roomId) this.reachedStart = res.payload.reached_start;
    } catch (e) {
      if (this.roomId === roomId) this.error = errorMessage(e);
    } finally {
      this.paginating = false;
    }
  }

  /** Send text; honors a pending reply or edit. */
  async send(body: string) {
    if (!this.roomId) return;
    const roomId = this.roomId;
    const text = body.trim();
    if (!text) return;
    try {
      if (this.editing?.event_id) {
        const target = this.editing.event_id;
        this.editing = null;
        await client.invoke({
          action: 'EditMessage',
          params: { room_id: roomId, event_id: target, body: text },
        });
      } else {
        const reply = this.replyTo?.event_id ?? null;
        this.replyTo = null;
        await client.invoke({
          action: 'SendText',
          params: { room_id: roomId, body: text, reply_to: reply, txn_id: null },
        });
      }
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /**
   * Sends a recorded clip as a voice message: the shell writes it to a temp file, the engine copies
   * it into the store and hands it to the connector.
   */
  async sendVoice(clip: VoiceClip) {
    const roomId = this.roomId;
    if (!roomId) return;
    const txnId = crypto.randomUUID();
    uploadsStore.begin({ txnId, roomId, name: 'Voice message' });
    try {
      const path = await writeTempFile(clip.blob, voiceExtension(clip.mimeType));
      await client.invoke({
        action: 'SendVoice',
        params: { room_id: roomId, file_path: path, duration_ms: clip.durationMs, txn_id: txnId },
      });
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
      await client.invoke({
        action: 'SendAttachment',
        params: {
          room_id: roomId,
          file_path: path,
          mime_type: mime ?? null,
          body: body ?? null,
          txn_id: txnId,
        },
      });
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      uploadsStore.end(txnId);
    }
  }

  async toggleReaction(item: TimelineItemDto, key: string) {
    if (!this.roomId || !item.event_id) return;
    if (this.capabilities && !this.capabilities.reactions) return;
    await client
      .invoke({
        action: 'ToggleReaction',
        params: { room_id: this.roomId, event_id: item.event_id, key },
      })
      .catch((e) => {
        this.error = errorMessage(e);
      });
  }

  /** Deletes messages for everyone; the account has to allow it (checked by the caller's UI). */
  async deleteItems(items: readonly TimelineItemDto[]) {
    const roomId = this.roomId;
    if (!roomId) return;
    for (const item of items) {
      if (!item.event_id) continue;
      try {
        await client.invoke({
          action: 'DeleteMessage',
          params: { room_id: roomId, event_id: item.event_id, for_everyone: true },
        });
      } catch (e) {
        this.error = errorMessage(e);
      }
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

  async retry(item: TimelineItemDto) {
    const txnId = transactionIdOf(item);
    if (!this.roomId || !txnId) return;
    await client
      .invoke({ action: 'RetrySend', params: { room_id: this.roomId, txn_id: txnId } })
      .catch((e) => {
        this.error = errorMessage(e);
      });
  }

  async cancel(item: TimelineItemDto) {
    const txnId = transactionIdOf(item);
    if (!this.roomId || !txnId) return;
    await client
      .invoke({ action: 'CancelSend', params: { room_id: this.roomId, txn_id: txnId } })
      .catch((e) => {
        this.error = errorMessage(e);
      });
  }

  /**
   * Publish a read position for the newest confirmed event.
   *
   * The position is only remembered once the engine confirms it: a failed call is left retryable and
   * reported, instead of being swallowed while the event id was already marked as done.
   */
  async markRead() {
    const roomId = this.roomId;
    if (!roomId) return;
    // A hidden window has not read anything, whatever the scroll position says.
    if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;

    const eventId = this.newestEventId();
    if (!eventId) return;
    if (eventId === this.lastReceiptEvent || eventId === this.receiptPending) {
      roomsStore.markRead(roomId);
      return;
    }

    this.receiptPending = eventId;
    try {
      await client.invoke({ action: 'MarkRead', params: { room_id: roomId, event_id: eventId } });
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
    if (!canEditItem(item, this.capabilities)) return;
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

/** Transaction id of a row that still has one, wherever the protocol put it. */
export function transactionIdOf(item: TimelineItemDto): string | null {
  if (item.transaction_id) return item.transaction_id;
  const state = item.send_state;
  if (state.status === 'Queued' || state.status === 'Sending') return state.data.transaction_id;
  return null;
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
