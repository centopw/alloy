import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('../ipc/index.js', () => ({ client: { invoke: mocks.invoke } }));
vi.mock('./timeline.svelte.js', () => ({
  timelineStore: { roomId: null, loading: false, jumpToEvent: vi.fn() },
}));

import { globalSearch, isSearchable, GLOBAL_SEARCH_LIMIT } from './globalSearch.svelte';
import { timelineStore } from './timeline.svelte';
import type { GlobalSearchHit } from '../generated/index.js';

const hit = (id: string): GlobalSearchHit => ({
  room_id: 'r1',
  room_name: 'R',
  event_id: id,
  sender: 'u1',
  sender_display_name: null,
  timestamp_millis: 1,
  body: id,
});

const found = (...ids: string[]) => ({
  type: 'AllMessagesFound' as const,
  payload: { hits: ids.map(hit) },
});

beforeEach(() => {
  vi.useFakeTimers();
  globalSearch.reset();
  mocks.invoke.mockReset();
  vi.mocked(timelineStore.jumpToEvent).mockReset();
});
afterEach(() => vi.useRealTimers());

describe('isSearchable', () => {
  it('needs three visible characters', () => {
    expect(isSearchable('ab')).toBe(false);
    expect(isSearchable('  ab ')).toBe(false);
    expect(isSearchable('abc')).toBe(true);
  });
});

describe('globalSearch', () => {
  it('waits for a pause in typing and searches once for the last query', async () => {
    mocks.invoke.mockResolvedValue(found('$1'));
    globalSearch.schedule('hel');
    globalSearch.schedule('hello');
    expect(mocks.invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(450);
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith({
      action: 'SearchAllMessages',
      params: { query: 'hello', limit: GLOBAL_SEARCH_LIMIT },
    });
    expect(globalSearch.hits.map((h) => h.event_id)).toEqual(['$1']);
    expect(globalSearch.query).toBe('hello');
    expect(globalSearch.loading).toBe(false);
  });

  it('clears results for a short query and ignores a slower earlier answer', async () => {
    const first = Promise.withResolvers<unknown>();
    mocks.invoke.mockReturnValueOnce(first.promise);
    globalSearch.schedule('first');
    await vi.advanceTimersByTimeAsync(450);
    globalSearch.schedule('ab');
    first.resolve(found('$late'));
    await vi.advanceTimersByTimeAsync(0);
    expect(globalSearch.hits).toEqual([]);
    expect(globalSearch.loading).toBe(false);
  });

  it('reports a failed search and stops loading', async () => {
    mocks.invoke.mockRejectedValue(new Error('index unavailable'));
    globalSearch.schedule('hello');
    await vi.advanceTimersByTimeAsync(450);
    expect(globalSearch.error).toBe('index unavailable');
    expect(globalSearch.loading).toBe(false);
  });

  it('opens the hit’s chat and jumps to the message once its timeline is open', async () => {
    const openRoom = vi.fn();
    timelineStore.roomId = 'r1';

    await globalSearch.open(hit('$9'), openRoom);

    expect(openRoom).toHaveBeenCalledWith('r1');
    expect(timelineStore.jumpToEvent).toHaveBeenCalledWith('$9');
  });

  it('forgets the query and the results on reset', async () => {
    mocks.invoke.mockResolvedValue(found('$1'));
    globalSearch.schedule('hello');
    await vi.advanceTimersByTimeAsync(450);
    globalSearch.reset();
    expect(globalSearch.hits).toEqual([]);
    expect(globalSearch.query).toBe('');
    expect(globalSearch.error).toBeNull();
  });
});
