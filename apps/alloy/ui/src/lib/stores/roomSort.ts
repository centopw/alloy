import type { RoomSummary, RoomTagDef } from '../generated/index.js';

/**
 * Room list orderings. `tag` orders rooms by the catalogue position of their first tag, with
 * untagged rooms last. Ties inside every ordering fall back to newest activity.
 */
export const ROOM_SORTS = ['activity', 'name', 'unread', 'tag'] as const;
export type RoomSort = (typeof ROOM_SORTS)[number];

export interface RoomSortContext {
  /** Unread count the list shows for a room (after the local read marks). */
  unreadOf: (room: RoomSummary) => number;
  /** Display title, used for name order and as a tie-breaker. */
  titleOf: (room: RoomSummary) => string;
  /** Tag ids in catalogue display order. */
  tagOrder: readonly string[];
}

export interface TagGroup {
  /** The tag this group shows, or `null` for rooms without any tag. */
  tag: RoomTagDef | null;
  rooms: RoomSummary[];
}

function byActivity(a: RoomSummary, b: RoomSummary, ctx: RoomSortContext): number {
  const newest = (r: RoomSummary) => r.latest_event?.timestamp_millis ?? 0;
  return newest(b) - newest(a) || ctx.titleOf(a).localeCompare(ctx.titleOf(b));
}

/** Lowest index among the room's tags, or -1 when none of them is in the index. */
function firstIndex(tags: readonly string[], index: ReadonlyMap<string, number>): number {
  let best = -1;
  for (const id of tags) {
    const i = index.get(id);
    if (i !== undefined && (best === -1 || i < best)) best = i;
  }
  return best;
}

/** Returns a new array in the requested order; the input is never mutated. */
export function sortRooms(
  rooms: readonly RoomSummary[],
  sort: RoomSort,
  ctx: RoomSortContext,
): RoomSummary[] {
  const list = [...rooms];
  switch (sort) {
    case 'activity':
      return list.sort((a, b) => byActivity(a, b, ctx));
    case 'name':
      return list.sort((a, b) => ctx.titleOf(a).localeCompare(ctx.titleOf(b)));
    case 'unread':
      return list.sort((a, b) => ctx.unreadOf(b) - ctx.unreadOf(a) || byActivity(a, b, ctx));
    case 'tag': {
      const index = new Map(ctx.tagOrder.map((id, i) => [id, i]));
      const rank = (r: RoomSummary) => {
        const i = firstIndex(r.tags, index);
        return i === -1 ? ctx.tagOrder.length : i;
      };
      return list.sort((a, b) => rank(a) - rank(b) || byActivity(a, b, ctx));
    }
  }
}

/**
 * Splits rooms into one group per catalogue tag, in catalogue order. A room with several tags
 * appears once, under its first tag. Empty groups are dropped; untagged rooms form a last group.
 * Keeps the order of `rooms` inside each group.
 */
export function groupRoomsByTag(rooms: readonly RoomSummary[], defs: readonly RoomTagDef[]): TagGroup[] {
  const index = new Map(defs.map((d, i) => [d.id, i]));
  const groups: TagGroup[] = defs.map((tag) => ({ tag, rooms: [] }));
  const untagged: RoomSummary[] = [];
  for (const r of rooms) {
    const i = firstIndex(r.tags, index);
    const group = i === -1 ? undefined : groups[i];
    if (group) group.rooms.push(r);
    else untagged.push(r);
  }
  const filled = groups.filter((g) => g.rooms.length > 0);
  return untagged.length > 0 ? [...filled, { tag: null, rooms: untagged }] : filled;
}
