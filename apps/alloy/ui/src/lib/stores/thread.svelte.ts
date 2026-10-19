import { client, applyDiff } from '../ipc/index.js';
import type { EngineEvent, TimelineItemDto } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { errorMessage, expect } from './errors.js';

type TimelineDiffEvent = Extract<EngineEvent, { type: 'TimelineDiff' }>;

/** Replies in a thread: every item except the root, which the panel shows on its own. */
export function threadReplies(items: TimelineItemDto[], rootEventId: string): TimelineItemDto[] {
  return items.filter((item) => item.event_id !== rootEventId);
}

/** The thread root as delivered by the engine, or null while it has not arrived yet. */
export function threadRootItem(items: TimelineItemDto[], rootEventId: string): TimelineItemDto | null {
  return items.find((item) => item.event_id === rootEventId) ?? null;
}

/**
 * The open thread panel: one thread's root plus its replies. Thread diffs arrive under their own
 * subscription id, so they never reach the room timeline store.
 */
class ThreadStore {
  roomId = $state<string | null>(null);
  rootEventId = $state<string | null>(null);
  items = $state<TimelineItemDto[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  private sub: Subscription<TimelineDiffEvent> | null = null;
  private batcher: FrameBatcher<TimelineDiffEvent> | null = null;
  private unlisten: (() => void) | null = null;
  /** Bumped on every open and close, so a late subscribe response from a superseded open is dropped. */
  private generation = 0;

  /** Open a thread (closing the previous one). */
  async open(roomId: string, rootEventId: string) {
    void this.close();
    const generation = ++this.generation;
    this.roomId = roomId;
    this.rootEventId = rootEventId;
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
      const res = expect(await client.subscribeThread(roomId, rootEventId), 'TimelineSubscribed');
      if (generation !== this.generation) {
        await client.unsubscribe(res.payload.subscription_id).catch(() => {});
        return;
      }
      sub.attach(res.payload.subscription_id);
    } catch (e) {
      if (generation !== this.generation) return;
      this.error = errorMessage(e);
      this.loading = false;
    }
  }

  /** Close the panel, releasing the thread subscription. State resets synchronously. */
  async close() {
    this.generation++;
    this.unlisten?.();
    this.unlisten = null;
    const id = this.sub?.subscriptionId;
    this.sub?.close();
    this.sub = null;
    this.batcher?.clear();
    this.batcher = null;
    this.items.length = 0;
    this.roomId = null;
    this.rootEventId = null;
    this.loading = false;
    this.error = null;
    if (id) await client.unsubscribe(id).catch(() => {});
  }

  /** Send a reply into the open thread. The engine threads it under the root. */
  async send(body: string, mentions: string[] = [], mentionRoom = false) {
    if (!this.roomId || !this.rootEventId) return;
    const text = body.trim();
    if (!text) return;
    try {
      await client.sendText(this.roomId, text, {
        markdown: true,
        in_thread_root: this.rootEventId,
        mentions: mentions.length ? mentions : null,
        mention_room: mentionRoom,
      });
    } catch (e) {
      this.error = errorMessage(e);
    }
  }
}

export const threadStore = new ThreadStore();
