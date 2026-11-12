import type { TimelineItemDto } from '$lib/generated';
import { mentionsAny } from '$lib/stores/mentions';

/** What newly appended rows add to the "jump to latest" badge while the user is scrolled up. */
export interface Arrivals {
  /** Messages from other people the user has not scrolled to yet. */
  count: number;
  /** Event ids of those that mention the user, oldest first. */
  mentions: string[];
}

/** The text of a message row; null for rows that carry none. */
function messageText(row: TimelineItemDto): string | null {
  const content = row.content;
  if (content.kind === 'TextMessage' || content.kind === 'Media') return content.data.body;
  return null;
}

/**
 * Counts appended rows that are someone else's messages, and which of them mention the user.
 * Mentions are plain `@Label` tokens in the message text, matched against the signed-in names.
 */
export function countArrivals(
  appended: readonly TimelineItemDto[],
  mentionLabels: readonly string[] = []
): Arrivals {
  const arrivals: Arrivals = { count: 0, mentions: [] };
  for (const row of appended) {
    if (row.is_own) continue;
    const text = messageText(row);
    if (text === null) continue;
    arrivals.count += 1;
    if (row.event_id && mentionsAny(text, mentionLabels)) arrivals.mentions.push(row.event_id);
  }
  return arrivals;
}

/** Badge text for a count: capped so it never outgrows the button. */
export function badgeLabel(count: number): string {
  return count > 99 ? '99+' : String(count);
}
