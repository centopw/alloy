import { describe, expect, it, vi } from 'vitest';
import { timelineStore, findRowIndexByEventId } from './timeline.svelte';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../ipc/index.js', () => ({
  client: { invoke: ipc.invoke },
  applyDiff: vi.fn(),
}));
vi.mock('./rooms.svelte.js', () => ({ roomsStore: { markRead: vi.fn(), activeCapabilities: null } }));
vi.mock('./uploads.svelte.js', () => ({ uploadsStore: { begin: vi.fn(), end: vi.fn() } }));

describe('loadOlder', () => {
  it('keeps the start-of-room answer of a room the user has already left', async () => {
    const answer = Promise.withResolvers<{ type: 'Paginated'; payload: { reached_start: boolean } }>();
    ipc.invoke.mockReturnValueOnce(answer.promise);
    timelineStore.roomId = 'short-chat';
    timelineStore.reachedStart = false;
    const pending = timelineStore.loadOlder();

    timelineStore.roomId = 'long-chat';
    answer.resolve({ type: 'Paginated', payload: { reached_start: true } });
    await pending;

    expect(timelineStore.reachedStart).toBe(false);
    expect(timelineStore.paginating).toBe(false);
  });
});

describe('findRowIndexByEventId', () => {
  const rows = [{ event_id: 'a' }, { event_id: null }, { event_id: 'b' }];

  it('returns the row index of a loaded event', () => {
    expect(findRowIndexByEventId(rows, 'b')).toBe(2);
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
