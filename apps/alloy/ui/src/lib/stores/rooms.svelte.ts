import { client, applyDiff } from '../ipc/index.js';
import type { EngineEvent, RoomSummary } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { expect } from './errors.js';
import { prefs } from './prefs.svelte.js';
import { roomNotify } from './roomNotify.svelte.js';
import { ui } from './ui.svelte.js';
import { markFor, unreadCount, type ReadMark } from './unread.js';
import { sortRooms } from './roomSort.js';
import { displayTags } from './tags.js';
import { tags } from './tags.svelte.js';

export type RoomFilter = 'all' | 'unread' | 'direct' | 'rooms';

type RoomDiffEvent = Extract<EngineEvent, { type: 'RoomListDiff' }>;

/** Live room list plus selection, search and filter chips. */
class RoomsStore {
  rooms = $state<RoomSummary[]>([]);
  activeRoomId = $state<string | null>(null);
  filter = $state<RoomFilter>('all');
  /** Tag id the list is restricted to; null shows every room. */
  tagFilter = $state<string | null>(null);
  query = $state('');
  loaded = $state(false);
  /** Joined spaces that contain each room, by room id (from the engine's space graph). */
  spaceParents = $state<Record<string, string[]>>({});
  /** Per room: the counter and newest message when the user last read it (see `unreadCount`). */
  private readMarks = $state<Record<string, ReadMark>>({});

  private sub: Subscription<RoomDiffEvent> | null = null;
  private batcher: FrameBatcher<RoomDiffEvent> | null = null;
  /** Last known summary of the open room; survives transient list resets. */
  private lastActive = $state<RoomSummary | null>(null);
  private unlisten: (() => void) | null = null;
  private spacesKey = '';
  private spacesTimer: ReturnType<typeof setTimeout> | null = null;

  visible = $derived.by(() => {
    const q = this.query.trim().toLowerCase();
    // A scope whose space is gone (left, or list reset) falls back to Home.
    const inScope = ui.scope !== 'home' && this.rooms.some((r) => r.id === ui.scope && r.is_space);
    // Home without "show all rooms" lists only rooms that belong to no joined space.
    const homeOnly = ui.scope === 'home' && !prefs.homeShowAllRooms;
    const listed = this.rooms.filter((r) => {
      if (r.membership !== 'Joined' && r.membership !== 'Invited') return false;
      if (r.is_space) return false;
      if (inScope && !(this.spaceParents[r.id] ?? []).includes(ui.scope)) return false;
      if (homeOnly && (this.spaceParents[r.id] ?? []).length > 0) return false;
      if (this.filter === 'unread' && this.unreadOf(r) === 0) return false;
      if (this.filter === 'direct' && !r.is_direct) return false;
      if (this.filter === 'rooms' && r.is_direct) return false;
      return !q || roomTitle(r).toLowerCase().includes(q);
    });
    const tagId = this.tagFilter;
    const tagged = tagId === null ? listed : listed.filter((r) => r.tags.includes(tagId));
    return sortRooms(tagged, prefs.roomSort, {
      unreadOf: (r) => this.unreadOf(r),
      titleOf: roomTitle,
      tagOrder: displayTags(tags.defs, this.rooms).map((d) => d.id),
    });
  });

  unreadTotal = $derived(this.rooms.reduce((n, r) => n + (this.unreadOf(r) > 0 ? 1 : 0), 0));

  /** Unread count to show for a room: the server's, minus what the user has already read here. */
  unreadOf(r: RoomSummary): number {
    return unreadCount(r, this.readMarks[r.id]);
  }

  /** Record that the user has read a room, so a stale server counter stops showing as unread. */
  markRead(roomId: string) {
    const r = this.rooms.find((x) => x.id === roomId);
    if (!r || r.unread_notifications.notification_count === 0) return;
    this.readMarks[roomId] = markFor(r);
  }

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
      this.scheduleSpaceRefresh();
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

  /** Re-read the space graph when the set of rooms/spaces changed (debounced). */
  private scheduleSpaceRefresh() {
    const key = this.rooms.map((r) => `${r.id}${r.is_space ? 's' : ''}${r.membership[0]}`).join('|');
    if (key === this.spacesKey) return;
    this.spacesKey = key;
    if (this.spacesTimer) clearTimeout(this.spacesTimer);
    this.spacesTimer = setTimeout(() => {
      this.spacesTimer = null;
      client
        .getSpaceParents()
        .then((res) => {
          const next: Record<string, string[]> = {};
          for (const p of expect(res, 'SpaceParents').payload.parents) next[p.room_id] = p.spaces;
          this.spaceParents = next;
        })
        .catch(() => {});
    }, 400);
  }

  async stop() {
    if (this.spacesTimer) clearTimeout(this.spacesTimer);
    this.spacesTimer = null;
    this.spacesKey = '';
    this.spaceParents = {};
    this.unlisten?.();
    roomNotify.clear();
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
