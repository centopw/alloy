import { client } from '../ipc/index.js';
import type { RoomMediaItem, RoomMediaKind } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';

/** Items requested per `GetRoomMedia` page. */
export const ROOM_MEDIA_PAGE_SIZE = 40;

export interface RoomMediaView {
  items: RoomMediaItem[];
  loading: boolean;
  /** Every page has been read; nothing more to load. */
  done: boolean;
  error: string | null;
}

interface Feed {
  items: RoomMediaItem[];
  /** Cursor for the next page, `null` when the server reported no more. */
  nextBatch: string | null;
  loaded: boolean;
  loading: boolean;
  error: string | null;
}

const EMPTY_FEED: Feed = { items: [], nextBatch: null, loaded: false, loading: false, error: null };

/** Keys are `roomId NUL kind`; room ids never contain NUL, so one room's keys share a prefix. */
const feedKey = (roomId: string, kind: RoomMediaKind) => `${roomId}\u0000${kind}`;

/**
 * Paged shared-media lists (media, files, links) per room and kind.
 * Responses for a room that was `reset` after the request started are dropped.
 */
export class RoomMediaStore {
  private feeds = $state<Record<string, Feed>>({});
  private inflight = new Map<string, Promise<void>>();
  /** Id of the request whose response is still wanted, per feed key. */
  private requests = new Map<string, number>();
  private nextRequest = 0;

  /** Current state of one list; an empty, idle view when nothing was requested. */
  view(roomId: string, kind: RoomMediaKind): RoomMediaView {
    const feed = this.feeds[feedKey(roomId, kind)] ?? EMPTY_FEED;
    return {
      items: feed.items.slice(),
      loading: feed.loading,
      done: feed.loaded && feed.nextBatch === null,
      error: feed.error,
    };
  }

  /** Reads the first page. No-op once loaded; concurrent calls share the request. */
  load(roomId: string, kind: RoomMediaKind): Promise<void> {
    const key = feedKey(roomId, kind);
    if (this.feeds[key]?.loaded) return Promise.resolve();
    return this.start(roomId, kind, null, false);
  }

  /** Appends the next page. No-op before the first page or after the last one. */
  loadMore(roomId: string, kind: RoomMediaKind): Promise<void> {
    const feed = this.feeds[feedKey(roomId, kind)];
    if (!feed?.loaded || feed.nextBatch === null) return Promise.resolve();
    return this.start(roomId, kind, feed.nextBatch, true);
  }

  /**
   * Re-reads the first page and prepends what arrived since the last read, keeping the pages
   * already loaded. A refresh failure leaves the list as it was: a stale preview is better than an
   * empty one, and the next event or reopen tries again.
   */
  refresh(roomId: string, kind: RoomMediaKind): Promise<void> {
    const key = feedKey(roomId, kind);
    if (!this.feeds[key]?.loaded) return this.load(roomId, kind);
    const pending = this.inflight.get(key);
    if (pending) return pending;

    const id = ++this.nextRequest;
    this.requests.set(key, id);
    const p: Promise<void> = (async () => {
      try {
        const res = await client.invoke({
          action: 'GetRoomMedia',
          params: { room_id: roomId, kind, from: null, limit: ROOM_MEDIA_PAGE_SIZE },
        });
        if (this.requests.get(key) !== id) return;
        const page = expect(res, 'RoomMedia').payload;
        const feed = this.feeds[key];
        const seen = new Set(feed.items.map((item) => item.event_id));
        const fresh = page.items.filter((item) => !seen.has(item.event_id));
        if (fresh.length > 0) this.setFeed(key, { items: [...fresh, ...feed.items] });
      } catch {
        // Keep the list; the refresh is opportunistic.
      }
    })().finally(() => {
      if (this.inflight.get(key) === p) this.inflight.delete(key);
    });
    this.inflight.set(key, p);
    return p;
  }

  /** Drops every list of a room; responses still in flight for it are ignored. */
  reset(roomId: string): void {
    const prefix = `${roomId}\u0000`;
    for (const key of Object.keys(this.feeds)) {
      if (!key.startsWith(prefix)) continue;
      delete this.feeds[key];
      this.requests.delete(key);
      this.inflight.delete(key);
    }
  }

  private start(
    roomId: string,
    kind: RoomMediaKind,
    from: string | null,
    append: boolean
  ): Promise<void> {
    const key = feedKey(roomId, kind);
    const pending = this.inflight.get(key);
    if (pending) return pending;

    const id = ++this.nextRequest;
    this.requests.set(key, id);
    this.setFeed(key, { loading: true, error: null });

    const p: Promise<void> = this.fetch(key, roomId, kind, from, append, id).finally(() => {
      if (this.inflight.get(key) === p) this.inflight.delete(key);
    });
    this.inflight.set(key, p);
    return p;
  }

  private async fetch(
    key: string,
    roomId: string,
    kind: RoomMediaKind,
    from: string | null,
    append: boolean,
    id: number
  ): Promise<void> {
    try {
      const res = await client.invoke({
        action: 'GetRoomMedia',
        params: { room_id: roomId, kind, from, limit: ROOM_MEDIA_PAGE_SIZE },
      });
      if (this.requests.get(key) !== id) return;
      const page = expect(res, 'RoomMedia').payload;
      const items = append ? this.feeds[key].items.slice() : [];
      const seen = new Set(items.map((item) => item.event_id));
      for (const item of page.items) {
        if (seen.has(item.event_id)) continue;
        seen.add(item.event_id);
        items.push(item);
      }
      this.setFeed(key, { items, nextBatch: page.next_batch, loaded: true, loading: false });
    } catch (err) {
      if (this.requests.get(key) === id) {
        this.setFeed(key, { loading: false, error: errorMessage(err) });
      }
    }
  }

  private setFeed(key: string, patch: Partial<Feed>): void {
    if (!this.feeds[key]) this.feeds[key] = { ...EMPTY_FEED, items: [] };
    Object.assign(this.feeds[key], patch);
  }
}

export const roomMedia = new RoomMediaStore();
