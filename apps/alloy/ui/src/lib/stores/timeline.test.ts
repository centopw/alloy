import { describe, expect, it } from 'vitest';
import { findRowIndexByEventId } from './timeline.svelte';

describe('findRowIndexByEventId', () => {
  const rows = [{ event_id: '$a' }, { event_id: null }, { event_id: '$b' }];

  it('returns the row index of a loaded event', () => {
    expect(findRowIndexByEventId(rows, '$b')).toBe(2);
  });

  it('returns -1 when the event is not loaded', () => {
    expect(findRowIndexByEventId(rows, '$missing')).toBe(-1);
  });

  it('returns -1 for a missing or empty id without matching local echoes', () => {
    expect(findRowIndexByEventId(rows, null)).toBe(-1);
    expect(findRowIndexByEventId(rows, undefined)).toBe(-1);
    expect(findRowIndexByEventId(rows, '')).toBe(-1);
  });
});
