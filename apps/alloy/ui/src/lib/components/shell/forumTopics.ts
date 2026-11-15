import type { RoomSummary } from '$lib/generated';

/**
 * A Telegram forum is a container whose topics are conversations of their own. The engine sends
 * each topic as a room with `parent_id` set to the forum's room id; this splits a visible room list
 * into the rows that sit at top level and the topics each forum owns.
 */
export interface ForumGroups {
  /** Rooms shown at top level: everything without a parent, plus topics whose forum is not listed. */
  top: RoomSummary[];
  /** Topics per forum room id, newest activity first. Only forums with topics have an entry. */
  topicsByForum: Map<string, RoomSummary[]>;
}

/**
 * Splits `rooms` into top-level rows and per-forum topic lists. Relative order of the top-level
 * rows is kept. A topic whose forum is absent from `rooms` (filtered out, or not yet synced) is
 * promoted to top level so it never disappears.
 */
export function groupForumTopics(rooms: readonly RoomSummary[]): ForumGroups {
  const listed = new Set(rooms.map((r) => r.id));
  const top: RoomSummary[] = [];
  const topicsByForum = new Map<string, RoomSummary[]>();
  for (const r of rooms) {
    const parent = r.parent_id;
    if (parent && listed.has(parent)) {
      const topics = topicsByForum.get(parent);
      if (topics) topics.push(r);
      else topicsByForum.set(parent, [r]);
    } else {
      top.push(r);
    }
  }
  // Newest activity first, then name: the same order the rest of the list uses.
  for (const topics of topicsByForum.values()) {
    topics.sort(
      (a, b) =>
        (b.latest_event?.timestamp_millis ?? 0) - (a.latest_event?.timestamp_millis ?? 0) ||
        (a.name ?? '').localeCompare(b.name ?? ''),
    );
  }
  return { top, topicsByForum };
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
