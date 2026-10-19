import type { RoomSummary } from '../generated/index.js';

/** What the room looked like when the user last read it. */
export interface ReadMark {
  /** The server-reported notification count at that moment. */
  count: number;
  /** Identifies the newest message at that moment (see `latestKey`). */
  latest: string;
}

export function latestKey(r: RoomSummary): string {
  const e = r.latest_event;
  return e ? `${e.timestamp_millis}:${e.sender}` : '';
}

export function markFor(r: RoomSummary): ReadMark {
  return { count: r.unread_notifications.notification_count, latest: latestKey(r) };
}

/**
 * Unread count to show for a room.
 *
 * Homeservers only resend a room's counters when something new happens in it, so after the user has
 * read a room the list can keep showing the old count. A read mark taken at that moment makes the
 * old count read: the room shows 0 until a newer message arrives, then only what came after.
 * A count that dropped below the mark means the server recounted, so it is shown as is.
 */
export function unreadCount(r: RoomSummary, mark: ReadMark | undefined): number {
  const raw = r.unread_notifications.notification_count;
  if (!mark || raw === 0) return raw;
  const newer = latestKey(r) !== mark.latest;
  if (raw > mark.count) return raw - mark.count;
  return newer ? raw : 0;
}

const STORAGE_KEY = 'alloy.readMarks';

/** Read marks survive a reload: the client recomputes counts from scratch on start, and the server's
 *  counters can lag, so without this a room you have read shows as unread again. */
export function loadReadMarks(storage: Pick<Storage, 'getItem'> | null): Record<string, ReadMark> {
  try {
    const raw = storage?.getItem(STORAGE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (!parsed || typeof parsed !== 'object') return {};
    const out: Record<string, ReadMark> = {};
    for (const [id, v] of Object.entries(parsed as Record<string, unknown>)) {
      const m = v as Partial<ReadMark>;
      if (typeof m.count === 'number' && typeof m.latest === 'string') out[id] = { count: m.count, latest: m.latest };
    }
    return out;
  } catch {
    return {};
  }
}

export function saveReadMarks(storage: Pick<Storage, 'setItem'> | null, marks: Record<string, ReadMark>) {
  try {
    storage?.setItem(STORAGE_KEY, JSON.stringify(marks));
  } catch {
    /* storage full or unavailable: marks then only last for this session */
  }
}
