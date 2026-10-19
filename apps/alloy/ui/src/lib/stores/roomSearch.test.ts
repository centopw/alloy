import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const engine = vi.hoisted(() => ({ invoke: vi.fn() }));
const timeline = vi.hoisted(() => ({
  roomId: null as string | null,
  items: [] as TestItem[],
  reachedStart: false,
  paginating: false,
  error: null as string | null,
  loadOlder: vi.fn(),
}));

vi.mock('../ipc/index.js', () => ({ client: { invoke: engine.invoke } }));
vi.mock('./timeline.svelte.js', () => ({ timelineStore: timeline }));
vi.mock('../components/timeline/ReplyQuote.svelte', () => ({
  quote: (item: { body: string }) => item.body,
}));

import { roomSearch } from './roomSearch.svelte.js';

interface TestItem {
  event_id: string;
  sender: string;
  sender_display_name: string | null;
  timestamp_millis: number;
  body: string;
}

function hit(event_id: string, body: string, sender = '@bob:alloy.chat'): TestItem {
  return { event_id, sender, sender_display_name: null, timestamp_millis: 1, body };
}

function serverPage(hits: TestItem[], next_batch: string | null = null, scope = 'Server') {
  return { type: 'RoomSearchResults', payload: { hits, next_batch, scope } };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
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
    timeline.reachedStart = false;
    timeline.error = null;
  });

  afterEach(() => {
    roomSearch.stop();
    vi.useRealTimers();
  });

  it('waits 250 ms after the last keystroke before asking the server', async () => {
    engine.invoke.mockResolvedValue(serverPage([]));
    roomSearch.bind('!debounce:alloy.chat');
    roomSearch.setQuery('b');
    await vi.advanceTimersByTimeAsync(100);
    roomSearch.setQuery('bo');
    await vi.advanceTimersByTimeAsync(249);
    expect(engine.invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(engine.invoke).toHaveBeenCalledTimes(1);
    expect(engine.invoke).toHaveBeenCalledWith({
      action: 'SearchRoomMessages',
      params: {
        room_id: '!debounce:alloy.chat',
        query: 'bo',
        sender: null,
        limit: 50,
        next_batch: null,
      },
    });
  });

  it('never sends an empty query', async () => {
    roomSearch.bind('!empty:alloy.chat');
    roomSearch.setQuery('   ');
    await vi.advanceTimersByTimeAsync(1000);
    expect(engine.invoke).not.toHaveBeenCalled();
    expect(roomSearch.mode).toBe('idle');
  });

  it('answers a sender-only filter from the loaded timeline without calling the server', async () => {
    timeline.roomId = '!sender:alloy.chat';
    timeline.items = [hit('$1', 'from bob', '@bob:alloy.chat'), hit('$2', 'from alice', '@alice:alloy.chat')];
    roomSearch.bind('!sender:alloy.chat');
    roomSearch.setSender('@bob:alloy.chat');
    await vi.advanceTimersByTimeAsync(1000);
    expect(engine.invoke).not.toHaveBeenCalled();
    expect(roomSearch.mode).toBe('local');
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$1']);
  });

  it('falls back to the loaded timeline when the server cannot search the room', async () => {
    timeline.roomId = '!enc:alloy.chat';
    timeline.items = [hit('$1', 'Hello there'), hit('$2', 'bye')];
    engine.invoke.mockResolvedValue(serverPage([], null, 'Unavailable'));
    roomSearch.bind('!enc:alloy.chat');
    roomSearch.setQuery('hello');
    await vi.advanceTimersByTimeAsync(250);
    await settle();
    expect(engine.invoke).toHaveBeenCalledTimes(1);
    expect(roomSearch.mode).toBe('local');
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$1']);
  });

  it('drops a server answer that a newer query has superseded', async () => {
    const first = deferred<unknown>();
    const second = deferred<unknown>();
    engine.invoke.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    roomSearch.bind('!stale:alloy.chat');
    roomSearch.setQuery('a');
    await vi.advanceTimersByTimeAsync(250);
    roomSearch.setQuery('ab');
    await vi.advanceTimersByTimeAsync(250);
    second.resolve(serverPage([hit('$new', 'ab new')]));
    await settle();
    first.resolve(serverPage([hit('$old', 'a old')]));
    await settle();
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$new']);
  });

  it('loadMore appends the next server page without repeating an event', async () => {
    engine.invoke
      .mockResolvedValueOnce(serverPage([hit('$1', 'x match'), hit('$2', 'x match 2')], 'page2'))
      .mockResolvedValueOnce(serverPage([hit('$2', 'x match 2'), hit('$3', 'x match 3')], null));
    roomSearch.bind('!more:alloy.chat');
    roomSearch.setQuery('x');
    await vi.advanceTimersByTimeAsync(250);
    await settle();
    expect(roomSearch.canLoadMore).toBe(true);
    await roomSearch.loadMore();
    expect(engine.invoke).toHaveBeenLastCalledWith({
      action: 'SearchRoomMessages',
      params: {
        room_id: '!more:alloy.chat',
        query: 'x',
        sender: null,
        limit: 50,
        next_batch: 'page2',
      },
    });
    expect(roomSearch.results.map((r) => r.event_id)).toEqual(['$1', '$2', '$3']);
    expect(roomSearch.canLoadMore).toBe(false);
  });

  it('clears the query and results when the room changes', async () => {
    engine.invoke.mockResolvedValue(serverPage([hit('$1', 'q')]));
    roomSearch.bind('!one:alloy.chat');
    roomSearch.setQuery('q');
    await vi.advanceTimersByTimeAsync(250);
    await settle();
    expect(roomSearch.results).toHaveLength(1);
    roomSearch.bind('!two:alloy.chat');
    expect(roomSearch.query).toBe('');
    expect(roomSearch.mode).toBe('idle');
    expect(roomSearch.results).toEqual([]);
  });
});
