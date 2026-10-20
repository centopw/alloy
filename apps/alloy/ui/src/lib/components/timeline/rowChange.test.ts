import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';
import { anchorsOf, detectRowChange } from './rowChange';

/** `d1`-style ids are day dividers, the rest are messages. */
const row = (id: string): TimelineItemDto =>
  ({ internal_id: id, content: { kind: /^d\d/.test(id) ? 'DayDivider' : 'TextMessage' } }) as TimelineItemDto;
const rows = (...ids: string[]) => ids.map(row);
const ids = (list: TimelineItemDto[]) => list.map((r) => r.internal_id);

describe('detectRowChange', () => {
  const before = anchorsOf(rows('b', 'c', 'd'));

  it('sees nothing in an unchanged list', () => {
    expect(detectRowChange(before, rows('b', 'c', 'd'))).toEqual({ prepended: false, arrived: [], replaced: false });
  });

  it('sees older history above the first row as a prepend that adds no new message', () => {
    const change = detectRowChange(before, rows('x', 'y', 'b', 'c', 'd'));
    expect(change.prepended).toBe(true);
    expect(change.arrived).toEqual([]);
  });

  it('sees history of the same day as a prepend even though the day divider stays first', () => {
    const open = anchorsOf(rows('d1', 'b', 'c'));
    const change = detectRowChange(open, rows('d1', 'x', 'y', 'b', 'c'));
    expect(change.prepended).toBe(true);
    expect(change.arrived).toEqual([]);
  });

  it('does not take a new day divider above the old first row for history', () => {
    const open = anchorsOf(rows('d1', 'b', 'c'));
    expect(detectRowChange(open, rows('d0', 'd1', 'b', 'c')).prepended).toBe(false);
  });

  it('counts only rows after the old last row as arrivals', () => {
    const change = detectRowChange(before, rows('b', 'c', 'd', 'e', 'f'));
    expect(change.prepended).toBe(false);
    expect(ids(change.arrived)).toEqual(['e', 'f']);
  });

  it('keeps a prepend and an arrival apart when both land in one update', () => {
    const change = detectRowChange(before, rows('x', 'b', 'c', 'd', 'e'));
    expect(change.prepended).toBe(true);
    expect(ids(change.arrived)).toEqual(['e']);
  });

  it('does not count rows inserted in the middle as arrivals', () => {
    const change = detectRowChange(before, rows('b', 'm', 'c', 'd'));
    expect(change).toEqual({ prepended: false, arrived: [], replaced: false });
  });

  it('reports a different list as replaced instead of as new messages', () => {
    expect(detectRowChange(before, rows('p', 'q', 'r', 's'))).toEqual({ prepended: false, arrived: [], replaced: true });
  });

  it('still finds the new tail when the old first row was removed', () => {
    const change = detectRowChange(before, rows('c', 'd', 'e'));
    expect(change.prepended).toBe(false);
    expect(ids(change.arrived)).toEqual(['e']);
  });

  it('has nothing to compare against before the first render', () => {
    expect(detectRowChange(null, rows('a', 'b'))).toEqual({ prepended: false, arrived: [], replaced: false });
  });

  it('has no anchors while only day dividers are shown', () => {
    expect(anchorsOf(rows('d1'))).toBeNull();
  });
});
