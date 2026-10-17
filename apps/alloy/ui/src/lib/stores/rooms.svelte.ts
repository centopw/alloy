import { client, applyDiff } from '../ipc/index.js';
import type { EngineEvent, RoomSummary } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { expect } from './errors.js';

export type RoomFilter = 'all' | 'unread' | 'direct' | 'rooms';

type RoomDiffEvent = Extract<EngineEvent, { type: 'RoomListDiff' }>;

/** Live room list plus selection, search and filter chips. */
class RoomsStore {
  rooms = $state<RoomSummary[]>([]);
  activeRoomId = $state<string | null>(null);
  filter = $state<RoomFilter>('all');
  query = $state('');
  loaded = $state(false);

  private sub: Subscription<RoomDiffEvent> | null = null;
  private batcher: FrameBatcher<RoomDiffEvent> | null = null;
  /** Last known summary of the open room; survives transient list resets. */
  private lastActive = $state<RoomSummary | null>(null);
  private unlisten: (() => void) | null = null;

  visible = $derived.by(() => {
    const q = this.query.trim().toLowerCase();
    return this.rooms.filter((r) => {
      if (r.membership !== 'Joined' && r.membership !== 'Invited') return false;
      if (r.is_space) return false;
      if (this.filter === 'unread' && r.unread_notifications.notification_count === 0) return false;
      if (this.filter === 'direct' && !r.is_direct) return false;
      if (this.filter === 'rooms' && r.is_direct) return false;
      return !q || roomTitle(r).toLowerCase().includes(q);
    });
  });

  unreadTotal = $derived(
    this.rooms.reduce((n, r) => n + (r.unread_notifications.notification_count > 0 ? 1 : 0), 0)
  );

  active = $derived(
    this.activeRoomId
      ? (this.rooms.find((r) => r.id === this.activeRoomId) ??
          (this.lastActive?.id === this.activeRoomId ? this.lastActive : null))
      : null
  );

  /** Start streaming the room list (call after login/restore). */
  async start() {
    void this.stop();
    this.loaded = false;
    const batcher = new FrameBatcher<RoomDiffEvent>((events) => {
      for (const e of events) applyDiff(this.rooms, e.payload.diff);
      const current = this.rooms.find((r) => r.id === this.activeRoomId);
      if (current) this.lastActive = current;
      this.loaded = true;
    });
    this.batcher = batcher;
    const sub = new Subscription<RoomDiffEvent>((e) => e.payload.subscription_id, (e) => batcher.push(e));
    this.sub = sub;
    this.unlisten = client.subscribe((e) => {
      if (e.type === 'RoomListDiff') sub.push(e);
    });
    const res = expect(await client.subscribeRoomList(null), 'RoomListSubscribed');
    sub.attach(res.payload.subscription_id);
  }

  async stop() {
    this.unlisten?.();
    this.unlisten = null;
    const id = this.sub?.subscriptionId;
    this.sub?.close();
    this.sub = null;
    this.batcher?.clear();
    this.batcher = null;
    this.rooms.length = 0;
    this.activeRoomId = null;
    this.lastActive = null;
    if (id) await client.unsubscribe(id).catch(() => {});
  }

  select(roomId: string | null) {
    this.activeRoomId = roomId;
    this.lastActive = this.rooms.find((r) => r.id === roomId) ?? null;
  }
}

export function roomTitle(r: RoomSummary): string {
  return r.name?.trim() || 'Empty room';
}

export const roomsStore = new RoomsStore();
