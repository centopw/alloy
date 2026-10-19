import { client, applyDiff } from '../ipc/index.js';
import type { EngineEvent, RoomSummary } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { expect } from './errors.js';
import { prefs } from './prefs.svelte.js';
import { roomNotify } from './roomNotify.svelte.js';
import { ui } from './ui.svelte.js';
import { loadReadMarks, markFor, saveReadMarks, unreadCount, type ReadMark } from './unread.js';
import { sortRooms } from './roomSort.js';
import { displayTags } from './tags.js';
import { tags } from './tags.svelte.js';

export type RoomFilter = 'all' | 'unread' | 'direct' | 'rooms' | 'archive';

/** Filter chips in display order. `label` is the chip text; `rooms` is the Channels chip. */
export const ROOM_FILTERS: readonly { value: RoomFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'direct', label: 'Direct' },
  { value: 'rooms', label: 'Channels' },
  { value: 'unread', label: 'Unread' },
  { value: 'archive', label: 'Archive' },
];

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
  private readMarks = $state<Record<string, ReadMark>>(loadReadMarks(typeof localStorage === 'undefined' ? null : localStorage));

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
      // Archived (low-priority) rooms live under their own chip and stay out of every other list.
      if (this.filter === 'archive' ? !r.low_priority : r.low_priority) return false;
      if (this.filter === 'unread' && !this.isUnread(r)) return false;
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

  unreadTotal = $derived(this.rooms.reduce((n, r) => n + (!r.low_priority && this.isUnread(r) ? 1 : 0), 0));

  /** Whether the room shows as unread: new messages, or marked unread by hand. */
  isUnread(r: RoomSummary): boolean {
    return this.unreadOf(r) > 0 || !!r.marked_unread;
  }

  /** Unread count to show for a room: the server's, minus what the user has already read here. */
  unreadOf(r: RoomSummary): number {
    return unreadCount(r, this.readMarks[r.id]);
  }

  private persistReadMarks() {
    saveReadMarks(typeof localStorage === 'undefined' ? null : localStorage, this.readMarks);
  }

  /** Forget every read mark (sign-out), so the next account starts clean. */
  clearReadMarks() {
    this.readMarks = {};
    this.persistReadMarks();
  }

  /** Record that the user has read a room, so a stale server counter stops showing as unread. */
  markRead(roomId: string) {
    const r = this.rooms.find((x) => x.id === roomId);
    if (!r || r.unread_notifications.notification_count === 0) return;
    this.readMarks[roomId] = markFor(r);
    this.persistReadMarks();
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
    // Opening a chat that was marked unread by hand reads it.
    if (roomId && this.lastActive?.marked_unread) void this.setMarkedUnread(roomId, false);
  }

  /** Pins a room to the top of the list (Matrix favourite tag). */
  setFavourite(roomId: string, favourite: boolean): Promise<void> {
    return this.#flag(roomId, { favourite }, () => client.setRoomFavourite(roomId, favourite));
  }

  /** Moves a room to or from the Archive (Matrix low-priority tag). */
  setArchived(roomId: string, archived: boolean): Promise<void> {
    return this.#flag(roomId, { low_priority: archived }, () => client.setRoomLowPriority(roomId, archived));
  }

  setMarkedUnread(roomId: string, unread: boolean): Promise<void> {
    return this.#flag(roomId, { marked_unread: unread }, () => client.setRoomMarkedUnread(roomId, unread));
  }

  /** Applies a flag at once and undoes it if the engine refuses; the room diff stays authoritative. */
  async #flag(roomId: string, patch: Partial<RoomSummary>, send: () => Promise<unknown>): Promise<void> {
    const index = this.rooms.findIndex((r) => r.id === roomId);
    if (index === -1) return;
    const previous = this.rooms[index];
    this.rooms[index] = { ...previous, ...patch };
    try {
      await send();
    } catch {
      const now = this.rooms.findIndex((r) => r.id === roomId);
      if (now !== -1) this.rooms[now] = { ...this.rooms[now], ...Object.fromEntries(Object.keys(patch).map((k) => [k, previous[k as keyof RoomSummary]])) };
    }
  }

  /**
   * Assigns or removes a tag on a room at once, so the list, tag chips and tag menu agree before the
   * engine answers. A refused write restores the previous tags; the engine's room diff stays authoritative.
   */
  async setRoomTag(roomId: string, tagId: string, enabled: boolean): Promise<boolean> {
    const room = this.rooms.find((r) => r.id === roomId);
    if (!room) return tags.setForRoom(roomId, tagId, enabled);
    const previous = [...room.tags];
    const next = enabled ? [...new Set([...previous, tagId])] : previous.filter((id) => id !== tagId);
    this.#setTags(roomId, next);
    const ok = await tags.setForRoom(roomId, tagId, enabled);
    if (!ok) this.#setTags(roomId, previous);
    return ok;
  }

  /** Replaces one room's tag list. Looked up by id each time because diffs replace entries by index. */
  #setTags(roomId: string, next: string[]) {
    const index = this.rooms.findIndex((r) => r.id === roomId);
    if (index !== -1) this.rooms[index] = { ...this.rooms[index], tags: next };
  }
}

export function roomTitle(r: RoomSummary): string {
  return r.name?.trim() || 'Empty room';
}

export const roomsStore = new RoomsStore();
