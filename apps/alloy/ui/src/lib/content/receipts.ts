import type { TimelineItemDto } from '$lib/generated';

/**
 * Who has read each of your messages. A read receipt sits on the newest event a person read, so a
 * person has read your message when their receipt is on it or on any later event.
 * Returns user ids per own event id; your own receipts are left out.
 */
export function readersByEvent(items: readonly TimelineItemDto[]): Map<string, string[]> {
  const me = items.find((i) => i.is_own)?.sender;
  const result = new Map<string, string[]>();
  const seen = new Set<string>();
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    for (const user of item.read_receipts ?? []) if (user !== me) seen.add(user);
    if (item.is_own && item.event_id) result.set(item.event_id, [...seen]);
  }
  return result;
}

/** Event id of your newest sent message, or null. */
export function lastOwnEventId(items: readonly TimelineItemDto[]): string | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.is_own && item.event_id) return item.event_id;
  }
  return null;
}
