import type { TimelineItemDto } from '$lib/generated';

/** What newly appended rows add to the "jump to latest" badge while the user is scrolled up. */
export interface Arrivals {
  /** Messages from other people the user has not scrolled to yet. */
  count: number;
  /** Event ids of those that mention the user, oldest first. */
  mentions: string[];
}

/** Counts appended rows that are someone else's messages, and which of them mention the user. */
export function countArrivals(appended: readonly TimelineItemDto[]): Arrivals {
  const arrivals: Arrivals = { count: 0, mentions: [] };
  for (const row of appended) {
    if (row.is_own) continue;
    const kind = row.content.kind;
    if (kind !== 'TextMessage' && kind !== 'Media' && kind !== 'Poll') continue;
    arrivals.count += 1;
    if (row.mentions_me && row.event_id) arrivals.mentions.push(row.event_id);
  }
  return arrivals;
}

/** Badge text for a count: capped so it never outgrows the button. */
export function badgeLabel(count: number): string {
  return count > 99 ? '99+' : String(count);
}
