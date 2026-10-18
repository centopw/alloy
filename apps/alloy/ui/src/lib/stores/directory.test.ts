import { beforeEach, describe, expect, it, vi } from 'vitest';
import { directoryStore } from './directory.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

const ACCOUNT = '@alice:example.org';

function room(id: string) {
  return {
    room_id: id,
    alias: null,
    name: id,
    topic: null,
    avatar_url: null,
    num_joined_members: 3,
    world_readable: false,
    guest_can_join: false,
    joined: false,
  };
}

function page(rooms: string[], next: string | null, total = rooms.length) {
  return {
    type: 'PublicRooms',
    payload: { rooms: rooms.map(room), next_batch: next, total_estimate: total },
  };
}

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

beforeEach(() => {
  invoke.mockReset();
  directoryStore.error = null;
});

describe('directoryStore', () => {
  it('sends a trimmed query with no cursor and records the page', async () => {
    invoke.mockResolvedValueOnce(page(['!a:x'], 'tok1', 42));
    await directoryStore.search(ACCOUNT, '  rust  ');

    expect(invoke).toHaveBeenCalledWith({
      action: 'SearchPublicRooms',
      params: { account_id: ACCOUNT, query: 'rust', server: null, since: null, limit: 20 },
    });
    expect(directoryStore.rooms.map((r) => r.room_id)).toEqual(['!a:x']);
    expect(directoryStore.totalEstimate).toBe(42);
    expect(directoryStore.hasMore).toBe(true);
    expect(directoryStore.loading).toBe(false);
  });

  it('loadMore appends the next page using the returned cursor', async () => {
    invoke.mockResolvedValueOnce(page(['!a:x'], 'tok1'));
    await directoryStore.search(ACCOUNT, '');
    invoke.mockResolvedValueOnce(page(['!b:x'], null));
    await directoryStore.loadMore(ACCOUNT);

    expect(invoke).toHaveBeenLastCalledWith({
      action: 'SearchPublicRooms',
      params: { account_id: ACCOUNT, query: null, server: null, since: 'tok1', limit: 20 },
    });
    expect(directoryStore.rooms.map((r) => r.room_id)).toEqual(['!a:x', '!b:x']);
    expect(directoryStore.hasMore).toBe(false);
  });

  it('ignores a stale response superseded by a newer search', async () => {
    const first = deferred<unknown>();
    invoke.mockReturnValueOnce(first.promise);
    const older = directoryStore.search(ACCOUNT, 'old');

    invoke.mockResolvedValueOnce(page(['!new:x'], null));
    await directoryStore.search(ACCOUNT, 'new');

    first.resolve(page(['!old:x'], 'stale'));
    await older;

    expect(directoryStore.query).toBe('new');
    expect(directoryStore.rooms.map((r) => r.room_id)).toEqual(['!new:x']);
    expect(directoryStore.hasMore).toBe(false);
  });

  it('records a failure message and stops loading', async () => {
    invoke.mockRejectedValueOnce(new Error('homeserver unreachable'));
    await directoryStore.search(ACCOUNT, 'x');

    expect(directoryStore.error).toBe('homeserver unreachable');
    expect(directoryStore.loading).toBe(false);
    expect(directoryStore.rooms).toEqual([]);
  });
});
