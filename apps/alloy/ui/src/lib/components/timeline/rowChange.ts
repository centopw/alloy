import type { TimelineItemDto } from '$lib/generated';

/** The first and last real row of the list as it was drawn last time, by stable id. */
export interface RowAnchors {
  first: string;
  last: string;
}

/** What changed between two renders of the timeline rows. */
export interface RowChange {
  /** Older rows were added above the first row that was shown before. */
  prepended: boolean;
  /** Rows that now follow the last row shown before; the only rows that can count as "new messages". */
  arrived: TimelineItemDto[];
  /** The rows shown before are gone (another room, a reload): nothing can be compared. */
  replaced: boolean;
}

/**
 * A day divider is a row of its own at the top of every day, so history of the same day loads below the
 * divider that is already first, and the first row of the list never changes. Anchors therefore skip dividers.
 */
function isDivider(row: TimelineItemDto): boolean {
  return row.content.kind === 'DayDivider';
}

/** The anchors to compare against on the next change; null while the list has no real row. */
export function anchorsOf(rows: readonly TimelineItemDto[]): RowAnchors | null {
  const first = rows.find((r) => !isDivider(r));
  const last = rows.findLast((r) => !isDivider(r));
  return first && last ? { first: first.internal_id, last: last.internal_id } : null;
}

/**
 * Tells growth at the top, growth at the bottom and a different list apart by locating the previous
 * first and last rows in the new list. Comparing lengths cannot: a history page loaded while scrolled
 * up, or rows inserted in the middle, also make the list longer without any new message.
 */
export function detectRowChange(prev: RowAnchors | null, rows: readonly TimelineItemDto[]): RowChange {
  if (!prev) return { prepended: false, arrived: [], replaced: false };
  const firstAt = rows.findIndex((r) => r.internal_id === prev.first);
  const lastAt = rows.findLastIndex((r) => r.internal_id === prev.last);
  if (firstAt < 0 && lastAt < 0) return { prepended: false, arrived: [], replaced: rows.length > 0 };
  return {
    prepended: firstAt > 0 && rows.slice(0, firstAt).some((r) => !isDivider(r)),
    arrived: lastAt >= 0 ? rows.slice(lastAt + 1) : [],
    replaced: false,
  };
}
