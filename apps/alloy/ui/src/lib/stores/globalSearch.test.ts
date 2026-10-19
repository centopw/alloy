import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  searchAllMessages: vi.fn(),
}));

vi.mock('../ipc/index.js', () => ({ client: { searchAllMessages: mocks.searchAllMessages } }));
vi.mock('./timeline.svelte.js', () => ({ timelineStore: { roomId: null, loading: false, jumpToEvent: vi.fn() } }));

import { globalSearch, isSearchable } from './globalSearch.svelte';

const hit = (id: string) => ({ room_id: '!r:hs', room_name: 'R', event_id: id, sender: '@a:hs', sender_display_name: null, timestamp_millis: 1, body: id });

beforeEach(() => {
  vi.useFakeTimers();
  globalSearch.reset();
  mocks.searchAllMessages.mockReset();
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
    mocks.searchAllMessages.mockResolvedValue({ type: 'AllMessagesFound', payload: { hits: [hit('$1')], next_batch: null } });
    globalSearch.schedule('hel');
    globalSearch.schedule('hello');
    expect(mocks.searchAllMessages).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(450);
    expect(mocks.searchAllMessages).toHaveBeenCalledTimes(1);
    expect(mocks.searchAllMessages).toHaveBeenCalledWith('hello', null);
    expect(globalSearch.hits.map((h) => h.event_id)).toEqual(['$1']);
  });

  it('clears results for a short query and ignores a slower earlier answer', async () => {
    let finishFirst: (v: unknown) => void = () => {};
    mocks.searchAllMessages.mockReturnValueOnce(new Promise((r) => (finishFirst = r)));
    globalSearch.schedule('first');
    await vi.advanceTimersByTimeAsync(450);
    globalSearch.schedule('ab');
    finishFirst({ type: 'AllMessagesFound', payload: { hits: [hit('$late')], next_batch: null } });
    await vi.advanceTimersByTimeAsync(0);
    expect(globalSearch.hits).toEqual([]);
    expect(globalSearch.loading).toBe(false);
  });

  it('appends the next page', async () => {
    mocks.searchAllMessages
      .mockResolvedValueOnce({ type: 'AllMessagesFound', payload: { hits: [hit('$1')], next_batch: 'b2' } })
      .mockResolvedValueOnce({ type: 'AllMessagesFound', payload: { hits: [hit('$2')], next_batch: null } });
    globalSearch.schedule('query');
    await vi.advanceTimersByTimeAsync(450);
    await globalSearch.more();
    expect(mocks.searchAllMessages).toHaveBeenLastCalledWith('query', 'b2');
    expect(globalSearch.hits.map((h) => h.event_id)).toEqual(['$1', '$2']);
    expect(globalSearch.nextBatch).toBeNull();
  });
});
