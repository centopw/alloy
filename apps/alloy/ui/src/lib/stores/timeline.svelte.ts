import { client, applyDiff } from '../ipc/index.js';
import type { EngineEvent, TimelineItemDto } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { errorMessage, expect } from './errors.js';
import { uploadsStore } from './uploads.svelte.js';

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

  private sub: Subscription<TimelineDiffEvent> | null = null;
  private batcher: FrameBatcher<TimelineDiffEvent> | null = null;
  private unlisten: (() => void) | null = null;
  private lastReceiptEvent: string | null = null;

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
    if (id) await client.unsubscribe(id).catch(() => {});
  }

  /** Load older history. */
  async loadOlder(count = 30) {
    if (!this.roomId || this.paginating || this.reachedStart) return;
    this.paginating = true;
    try {
      const res = expect(await client.paginateBackwards(this.roomId, count), 'Paginated');
      this.reachedStart = res.payload.reached_start;
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.paginating = false;
    }
  }

  /** Send text (markdown by default); honors a pending reply or edit. */
  async send(body: string, mentions: string[] = []) {
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
          mentions: mentions.length ? mentions : null
        });
      }
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  async sendFile(path: string, mime?: string | null) {
    if (!this.roomId) return;
    const roomId = this.roomId;
    const txnId = crypto.randomUUID();
    uploadsStore.begin({ txnId, roomId, name: path.split(/[\\/]/).pop() || path });
    try {
      await client.sendAttachment(roomId, path, { mime_type: mime ?? null, txn_id: txnId });
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

  /** Report the newest confirmed event as read (call when the user is at the bottom). */
  async markRead() {
    if (!this.roomId) return;
    for (let i = this.items.length - 1; i >= 0; i--) {
      const id = this.items[i].event_id;
      if (id) {
        if (id !== this.lastReceiptEvent) {
          this.lastReceiptEvent = id;
          await client.markRead(this.roomId, id).catch(() => {});
        }
        return;
      }
    }
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
}

export const timelineStore = new TimelineStore();
