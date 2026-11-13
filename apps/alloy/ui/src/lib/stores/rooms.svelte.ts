import { client, applyDiff } from '../ipc/index.js';
import type { Account, CapabilitiesDto, EngineEvent, Network, RoomSummary } from '../generated/index.js';
import { FrameBatcher, Subscription } from './subscription.js';
import { expect } from './errors.js';
import { prefs } from './prefs.svelte.js';
import { loadReadMarks, markFor, saveReadMarks, unreadCount, type ReadMark } from './unread.js';
import { sortRooms } from './roomSort.js';
import { displayTags } from './tags.js';
import { tags } from './tags.svelte.js';
import { draftsStore } from './drafts.svelte.js';

import { t } from '$lib/i18n';

export type RoomFilter = 'all' | 'unread' | 'direct' | 'rooms' | 'archive';

/** Filter chips in display order. `label` is the chip text; `rooms` is the Channels chip. */
export const ROOM_FILTERS: readonly { value: RoomFilter; label: string }[] = [
  { value: 'all', label: t('room-filter-all') },
  { value: 'direct', label: t('room-filter-direct') },
  { value: 'rooms', label: t('room-filter-channels') },
  { value: 'unread', label: t('room-filter-unread') },
  { value: 'archive', label: t('room-filter-archive') },
];

/**
 * `muted_until` value the engine uses for "muted forever" (`u64::MAX` in the protocol). It is far
 * beyond any real timestamp, so no date comparison can un-mute the chat by accident.
 */
export const MUTE_FOREVER = 18_446_744_073_709_551_615;

/** True while the network's `muted_until` is in the future (or forever). */
export function isRoomMuted(room: Pick<RoomSummary, 'muted_until'>, now = Date.now()): boolean {
  const until = room.muted_until;
  return until !== null && (until >= MUTE_FOREVER || until > now);
}

/** Two-letter network badge shown before a chat name (text label, never an icon — ADR 0001). */
export function networkBadge(network: Network): string {
  switch (network) {
    case 'telegram':
      return 'TG';
    case 'messenger':
      return 'FB';
    case 'fake':
      return 'FX';
    default:
      return network;
  }
}

/** Full network name for labels and tooltips. */
export function networkName(network: Network): string {
  switch (network) {
    case 'telegram':
      return t('chat-network-telegram');
    case 'messenger':
      return t('chat-network-messenger');
    case 'fake':
      return t('chat-network-fake');
    default:
      return network;
  }
}

/** One account and the chats it owns, in list order. */
export interface AccountGroup {
  account: Account | null;
  rooms: RoomSummary[];
}

type RoomDiffEvent = Extract<EngineEvent, { type: 'RoomListDiff' }>;

/** Live room list plus selection, search and filter chips. */
class RoomsStore {
  rooms = $state<RoomSummary[]>([]);
  /** Every locally stored account, refreshed from `ListAccounts`/`AccountsChanged`. */
  accounts = $state<Account[]>([]);
  activeRoomId = $state<string | null>(null);
  filter = $state<RoomFilter>('all');
  /** Tag id the list is restricted to; null shows every room. */
  tagFilter = $state<string | null>(null);
  query = $state('');
  loaded = $state(false);
  /** Per room: the counter and newest message when the user last read it (see `unreadCount`). */
  private readMarks = $state<Record<string, ReadMark>>(loadReadMarks(typeof localStorage === 'undefined' ? null : localStorage));

  private sub: Subscription<RoomDiffEvent> | null = null;
  private batcher: FrameBatcher<RoomDiffEvent> | null = null;
  /** Last known summary of the open room; survives transient list resets. */
  private lastActive = $state<RoomSummary | null>(null);
  private unlisten: (() => void) | null = null;

  visible = $derived.by(() => {
    const q = this.query.trim().toLowerCase();
    const listed = this.rooms.filter((r) => {
      // Archived chats live under their own chip and stay out of every other list.
      if (this.filter === 'archive' ? !r.archived : r.archived) return false;
      if (this.filter === 'unread' && !this.isUnread(r)) return false;
      if (this.filter === 'direct' && !r.is_direct) return false;
      if (this.filter === 'rooms' && r.is_direct) return false;
      return !q || roomTitle(r).toLowerCase().includes(q);
    });
    const tagId = this.tagFilter;
    const tagged = tagId === null ? listed : listed.filter((r) => r.tags.includes(tagId));
    // Pinned chats sit above everything else inside the active view.
    const sorted = sortRooms(tagged, prefs.roomSort, {
      unreadOf: (r) => this.unreadOf(r),
      titleOf: roomTitle,
      tagOrder: displayTags(tags.defs, this.rooms).map((d) => d.id),
    });
    return [...sorted.filter((r) => r.pinned), ...sorted.filter((r) => !r.pinned)];
  });

  /** The visible chats split per account, in account order; empty groups are dropped. */
  accountGroups = $derived.by((): AccountGroup[] => {
    if (this.accounts.length <= 1) return [];
    const byId = new Map(this.accounts.map((a) => [a.id, a] as const));
    const groups = new Map<string, AccountGroup>();
    for (const id of byId.keys()) groups.set(id, { account: byId.get(id) ?? null, rooms: [] });
    const orphan: AccountGroup = { account: null, rooms: [] };
    for (const room of this.visible) {
      const group = groups.get(room.account_id);
      if (group) group.rooms.push(room);
      else orphan.rooms.push(room);
    }
    const filled = [...groups.values()].filter((g) => g.rooms.length > 0);
    return orphan.rooms.length > 0 ? [...filled, orphan] : filled;
  });

  unreadTotal = $derived(this.rooms.reduce((n, r) => n + (!r.archived && this.isUnread(r) ? 1 : 0), 0));

  /** Whether the room shows as unread: new messages, or marked unread by hand. */
  isUnread(r: RoomSummary): boolean {
    return this.unreadOf(r) > 0 || !!r.marked_unread;
  }

  /** Unread count to show for a room: the engine's, minus what the user has already read here. */
  unreadOf(r: RoomSummary): number {
    return unreadCount(r, this.readMarks[r.id]);
  }

  active = $derived(
    this.activeRoomId
      ? (this.rooms.find((r) => r.id === this.activeRoomId) ??
          (this.lastActive?.id === this.activeRoomId ? this.lastActive : null))
      : null
  );

  /** What the account behind a chat lets the UI do. */
  capabilitiesFor(accountId: string): CapabilitiesDto | null {
    return this.accounts.find((a) => a.id === accountId)?.capabilities ?? null;
  }

  /** Capabilities of the open chat's account. */
  activeCapabilities = $derived(
    this.active ? (this.capabilitiesFor(this.active.account_id) ?? null) : null
  );

  /** The account that owns the open chat. */
  activeAccount = $derived(
    this.active ? (this.accounts.find((a) => a.id === this.active?.account_id) ?? null) : null
  );

  private persistReadMarks() {
    saveReadMarks(typeof localStorage === 'undefined' ? null : localStorage, this.readMarks);
  }

  /** Forget every read mark (sign-out), so the next account starts clean. */
  clearReadMarks() {
    this.readMarks = {};
    this.persistReadMarks();
  }

  /** Record that the user has read a room and tell the engine its read position moved. */
  markRead(roomId: string) {
    const r = this.rooms.find((x) => x.id === roomId);
    if (!r) return;
    if (r.unread_notifications.notification_count > 0) {
      this.readMarks[roomId] = markFor(r);
      this.persistReadMarks();
    }
    void client.invoke({ action: 'MarkRoomRead', params: { room_id: roomId } }).catch(() => {
      /* the room list stream carries the real counters; nothing to roll back */
    });
  }

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
      else if (e.type === 'AccountsChanged') this.accounts = e.payload.accounts;
    });
    const [res] = await Promise.all([client.invoke({ action: 'SubscribeRoomList', params: { filter: null } }), this.loadAccounts()]);
    sub.attach(expect(res, 'RoomListSubscribed').payload.subscription_id);
  }

  /** Reads the account list; failures leave the previous list alone (the stream resends it). */
  async loadAccounts(): Promise<void> {
    try {
      const res = await client.invoke({ action: 'ListAccounts' });
      this.accounts = expect(res, 'AccountsList').payload.accounts;
    } catch {
      /* the account list is decoration for the room list; a failed read is retried on the next event */
    }
  }

  async stop() {
    this.unlisten?.();
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

  /**
   * Rebuilds the room list subscription (the manual Refresh) and keeps the room that is open selected.
   * `stop()` clears the selection and the list, so it is put back as soon as the restart has begun.
   */
  async restart() {
    const activeId = this.activeRoomId;
    const last = this.lastActive;
    const started = this.start();
    this.activeRoomId = activeId;
    this.lastActive = last;
    await started;
  }

  select(roomId: string | null) {
    this.activeRoomId = roomId;
    this.lastActive = this.rooms.find((r) => r.id === roomId) ?? null;
    // Opening a chat that was marked unread by hand reads it.
    if (roomId && this.lastActive?.marked_unread) void this.setMarkedUnread(roomId, false);
  }

  /** The draft text a row shows: the unsaved editor state wins over the stored preview. */
  draftOf(room: RoomSummary): string | null {
    return draftsStore.previewOf(room.id) ?? room.draft ?? null;
  }

  /** Pins or unpins a chat in the list. */
  setPinned(roomId: string, pinned: boolean): Promise<void> {
    return this.#flag(roomId, { pinned }, () =>
      client.invoke({ action: 'SetRoomPinned', params: { room_id: roomId, pinned } })
    );
  }

  /** Moves a chat to or from the Archive. */
  setArchived(roomId: string, archived: boolean): Promise<void> {
    return this.#flag(roomId, { archived }, () =>
      client.invoke({ action: 'SetRoomArchived', params: { room_id: roomId, archived } })
    );
  }

  setMarkedUnread(roomId: string, unread: boolean): Promise<void> {
    return this.#flag(roomId, { marked_unread: unread }, () =>
      client.invoke({ action: 'SetRoomMarkedUnread', params: { room_id: roomId, unread } })
    );
  }

  /** Mutes until `mutedUntil` (epoch ms, or `null` to unmute). */
  setMuted(roomId: string, mutedUntil: number | null): Promise<void> {
    return this.#flag(roomId, { muted_until: mutedUntil }, () =>
      client.invoke({ action: 'SetRoomMuted', params: { room_id: roomId, muted_until: mutedUntil } })
    );
  }

  /** Applies a flag at once and undoes it if the engine refuses; the room diff stays authoritative. */
  async #flag(
    roomId: string,
    patch: Partial<Pick<RoomSummary, 'pinned' | 'archived' | 'marked_unread' | 'muted_until'>>,
    send: () => Promise<unknown>
  ): Promise<void> {
    const index = this.rooms.findIndex((r) => r.id === roomId);
    if (index === -1) return;
    const previous = this.rooms[index];
    this.rooms[index] = { ...previous, ...patch };
    try {
      await send();
    } catch {
      const now = this.rooms.findIndex((r) => r.id === roomId);
      if (now !== -1) {
        this.rooms[now] = {
          ...this.rooms[now],
          ...Object.fromEntries(Object.keys(patch).map((k) => [k, previous[k as keyof RoomSummary]])),
        };
      }
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

/** Display title of a chat: its name, or a neutral placeholder for an unnamed chat. */
export function roomTitle(r: RoomSummary): string {
  return r.name?.trim() || t('chat-untitled-room');
}

export const roomsStore = new RoomsStore();
