import type { RoomSummary } from '$lib/generated';

/**
 * A Telegram forum is a container whose topics are conversations of their own. The engine sends
 * each topic as a room with `parent_id` set to the forum's room id, so the list shows one row per
 * forum and the topics live beside the open conversation.
 */

/** Newest activity first, then name: the order the room list and the topic rail share. */
function byActivityThenName(a: RoomSummary, b: RoomSummary): number {
  return (
    (b.latest_event?.timestamp_millis ?? 0) - (a.latest_event?.timestamp_millis ?? 0) ||
    (a.name ?? '').localeCompare(b.name ?? '')
  );
}

/**
 * Every topic a forum owns, newest activity first, so `[0]` is the topic the forum opens. A topic
 * with no messages sorts after the ones that have some. Topics are conversations of their own, so
 * this looks at the whole room list rather than at the filtered sidebar view.
 */
export function topicsOf(rooms: readonly RoomSummary[], forumId: string): RoomSummary[] {
  return rooms.filter((r) => r.parent_id === forumId).sort(byActivityThenName);
}

/** How many topics a forum has; the chat header shows this instead of a member count. */
export function forumTopicCount(rooms: readonly RoomSummary[], forumId: string): number {
  return topicsOf(rooms, forumId).length;
}

/**
 * Unread totals a forum row shows: its own counters plus those of its topics. `unreadOf` is the
 * read-mark-aware notification count the store supplies; highlights come from the engine.
 */
export function forumTotals(
  forum: RoomSummary,
  topics: readonly RoomSummary[],
  unreadOf: (room: RoomSummary) => number,
): { notifications: number; highlights: number } {
  let notifications = unreadOf(forum);
  let highlights = forum.unread_notifications.highlight_count;
  for (const topic of topics) {
    notifications += unreadOf(topic);
    highlights += topic.unread_notifications.highlight_count;
  }
  return { notifications, highlights };
}
