import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const engine = vi.hoisted(() => ({ invoke: vi.fn() }));
const timeline = vi.hoisted(() => ({
  roomId: null as string | null,
  items: [] as TestItem[],
}));

vi.mock('../ipc/index.js', () => ({ client: { invoke: engine.invoke } }));
vi.mock('./timeline.svelte.js', () => ({ timelineStore: timeline }));
vi.mock('../components/timeline/ReplyQuote.svelte', () => ({
  quote: (item: { body: string }) => item.body,
}));
vi.mock('../content/searchHighlight.js', () => ({ matchesQuery: () => true }));

import { ROOM_SEARCH_LIMIT, roomSearch } from './roomSearch.svelte.js';

interface TestItem {
  event_id: string;
  sender: string;
  sender_display_name: string | null;
  timestamp_millis: number;
  body: string;
}

function hit(event_id: string, body: string, sender = 'bob'): TestItem {
  return { event_id, sender, sender_display_name: null, timestamp_millis: 1, body };
}

function results(hits: TestItem[]) {
  return { type: 'RoomSearchResults', payload: { hits } };
}

function deferred<T>() {
  const { promise, resolve } = Promise.withResolvers<T>();
  return { promise, resolve };
}

/** Lets resolved engine promises run their continuations. */
async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

describe('roomSearch', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    engine.invoke.mockReset();
    timeline.roomId = null;
    timeline.items = [];
  });

  afterEach(() => {
    roomSearch.stop();
    vi.useRealTimers();
  });

  it('waits 250 ms after the last keystroke before asking the local index', async () => {
    engine.invoke.mockResolvedValue(results([]));
    roomSearch.bind('chat-1');
    roomSearch.setQuery('b');
    await vi.advanceTimersByTimeAsync(100);
    roomSearch.setQuery('bo');
    await vi.advanceTimersByTimeAsync(249);
    expect(engine.invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(engine.invoke).toHaveBeenCalledTimes(1);
    expect(engine.invoke).toHaveBeenCalledWith({
      action: 'SearchRoomMessages',
      params: { room_id: 'chat-1', query: 'bo', limit: ROOM_SEARCH_LIMIT },
    });
  });

  it('never sends an empty query', async () => {
    roomSearch.bind('chat-1');
    roomSearch.setQuery('   ');
    await vi.advanceTimersByTimeAsync(1000);
    expect(engine.invoke).not.toHaveBeenCalled();
    expect(roomSearch.mode).toBe('idle');
  });

  it('answers a sender-only filter from the loaded timeline without calling the engine', async () => {
    timeline.roomId = 'chat-2';
    timeline.items = [hit('$1', 'from bob', 'bob'), hit('$2', 'from alice', 'alice')];
    roomSearch.bind('chat-2');
    roomSearch.setSender('bob');
    await vi.advanceTimersByTimeAsync(1000);
    expect(engine.invoke).not.toHaveBeenCalled();
    expect(roomSearch.mode).toBe('local');
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$1']);
  });

  it('narrows text results by the chosen sender without a second request', async () => {
    engine.invoke.mockResolvedValue(
      results([hit('$1', 'a', 'bob'), hit('$2', 'ab', 'alice')])
    );
    roomSearch.bind('chat-3');
    roomSearch.setQuery('a');
    await vi.advanceTimersByTimeAsync(250);
    await settle();
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$1', '$2']);

    roomSearch.setSender('bob');
    await vi.advanceTimersByTimeAsync(250);
    await settle();
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$1']);
  });

  it('drops an answer that a newer query has superseded', async () => {
    const first = deferred<unknown>();
    const second = deferred<unknown>();
    engine.invoke.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    roomSearch.bind('chat-4');
    roomSearch.setQuery('a');
    await vi.advanceTimersByTimeAsync(250);
    roomSearch.setQuery('ab');
    await vi.advanceTimersByTimeAsync(250);
    second.resolve(results([hit('$new', 'ab new')]));
    await settle();
    first.resolve(results([hit('$old', 'a old')]));
    await settle();
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$new']);
  });

  it('clears the query and results when the room changes', async () => {
    engine.invoke.mockResolvedValue(results([hit('$1', 'q')]));
    roomSearch.bind('chat-5');
    roomSearch.setQuery('q');
    await vi.advanceTimersByTimeAsync(250);
    await settle();
    expect(roomSearch.results).toHaveLength(1);
    roomSearch.bind('chat-6');
    expect(roomSearch.query).toBe('');
    expect(roomSearch.mode).toBe('idle');
    expect(roomSearch.results).toEqual([]);
  });

  it('reports an engine failure and keeps the panel usable', async () => {
    engine.invoke.mockRejectedValueOnce({ code: 'internal_error', message: 'index unavailable' });
    roomSearch.bind('chat-7');
    roomSearch.setQuery('hello');
    await vi.advanceTimersByTimeAsync(250);
    await settle();
    expect(roomSearch.error).toBe('index unavailable');
    expect(roomSearch.loading).toBe(false);
  });
});
