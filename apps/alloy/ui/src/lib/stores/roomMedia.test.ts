import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ROOM_MEDIA_PAGE_SIZE, RoomMediaStore } from './roomMedia.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

const ROOM = '!general:alloy.chat';

function item(id: string) {
  return {
    event_id: id,
    sender: '@alice:alloy.chat',
    sender_display_name: 'Alice',
    timestamp_millis: 0,
    kind: 'image',
    body: id,
    uri: null,
    thumbnail_uri: null,
    mime_type: null,
    size_bytes: null,
    width: null,
    height: null,
  };
}

function page(ids: string[], next: string | null) {
  return {
    type: 'RoomMedia',
    payload: { room_id: ROOM, items: ids.map(item), next_batch: next },
  };
}

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

let store: RoomMediaStore;

beforeEach(() => {
  invoke.mockReset();
  store = new RoomMediaStore();
});

describe('RoomMediaStore', () => {
  it('returns an empty idle view for a list that was never requested', () => {
    expect(store.view(ROOM, 'Media')).toEqual({ items: [], loading: false, done: false, error: null });
  });

  it('reads the first page with the page size and no cursor', async () => {
    invoke.mockResolvedValueOnce(page(['a', 'b'], null));
    await store.load(ROOM, 'Media');

    expect(invoke).toHaveBeenCalledWith({
      action: 'GetRoomMedia',
      params: { room_id: ROOM, kind: 'Media', from: null, limit: ROOM_MEDIA_PAGE_SIZE },
    });
    expect(ROOM_MEDIA_PAGE_SIZE).toBe(40);
    const view = store.view(ROOM, 'Media');
    expect(view.items.map((i) => i.event_id)).toEqual(['a', 'b']);
    expect(view.done).toBe(true);
    expect(view.loading).toBe(false);
  });

  it('does not re-read a loaded list', async () => {
    invoke.mockResolvedValueOnce(page(['a'], null));
    await store.load(ROOM, 'Media');
    await store.load(ROOM, 'Media');

    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('shares one request between concurrent loads', async () => {
    const d = deferred<unknown>();
    invoke.mockReturnValueOnce(d.promise);

    const a = store.load(ROOM, 'Files');
    const b = store.load(ROOM, 'Files');
    expect(store.view(ROOM, 'Files').loading).toBe(true);
    expect(invoke).toHaveBeenCalledTimes(1);

    d.resolve(page(['f1'], null));
    await Promise.all([a, b]);
    expect(store.view(ROOM, 'Files').items).toHaveLength(1);
  });

  it('keeps each kind in its own list', async () => {
    invoke.mockResolvedValueOnce(page(['m1'], null)).mockResolvedValueOnce(page(['l1'], null));
    await store.load(ROOM, 'Media');
    await store.load(ROOM, 'Links');

    expect(store.view(ROOM, 'Media').items.map((i) => i.event_id)).toEqual(['m1']);
    expect(store.view(ROOM, 'Links').items.map((i) => i.event_id)).toEqual(['l1']);
  });

  it('appends the next page using next_batch as the cursor', async () => {
    invoke.mockResolvedValueOnce(page(['a'], 'cursor-1'));
    await store.load(ROOM, 'Media');
    expect(store.view(ROOM, 'Media').done).toBe(false);

    invoke.mockResolvedValueOnce(page(['b'], null));
    await store.loadMore(ROOM, 'Media');

    expect(invoke).toHaveBeenLastCalledWith({
      action: 'GetRoomMedia',
      params: { room_id: ROOM, kind: 'Media', from: 'cursor-1', limit: ROOM_MEDIA_PAGE_SIZE },
    });
    expect(store.view(ROOM, 'Media').items.map((i) => i.event_id)).toEqual(['a', 'b']);
    expect(store.view(ROOM, 'Media').done).toBe(true);
  });

  it('does nothing for loadMore before the first page or after the last one', async () => {
    await store.loadMore(ROOM, 'Media');
    expect(invoke).not.toHaveBeenCalled();

    invoke.mockResolvedValueOnce(page(['a'], null));
    await store.load(ROOM, 'Media');
    await store.loadMore(ROOM, 'Media');
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('drops items already seen in an earlier page', async () => {
    invoke.mockResolvedValueOnce(page(['a', 'b'], 'next'));
    await store.load(ROOM, 'Media');

    invoke.mockResolvedValueOnce(page(['b', 'c'], null));
    await store.loadMore(ROOM, 'Media');

    expect(store.view(ROOM, 'Media').items.map((i) => i.event_id)).toEqual(['a', 'b', 'c']);
  });

  it('ignores a response that arrives after the room was reset', async () => {
    const d = deferred<unknown>();
    invoke.mockReturnValueOnce(d.promise);
    const pending = store.load(ROOM, 'Media');

    store.reset(ROOM);
    d.resolve(page(['stale'], null));
    await pending;

    expect(store.view(ROOM, 'Media')).toEqual({ items: [], loading: false, done: false, error: null });
  });

  it('reset clears only the given room', async () => {
    invoke.mockResolvedValueOnce(page(['a'], null)).mockResolvedValueOnce(page(['b'], null));
    await store.load(ROOM, 'Media');
    await store.load('!other:alloy.chat', 'Media');

    store.reset(ROOM);

    expect(store.view(ROOM, 'Media').items).toEqual([]);
    expect(store.view('!other:alloy.chat', 'Media').items).toHaveLength(1);
  });

  it('prepends media that arrived since the first read, keeping the pages already loaded', async () => {
    invoke.mockResolvedValueOnce(page(['m5', 'm4'], 'cursor1'));
    await store.load(ROOM, 'Media');
    invoke.mockResolvedValueOnce(page(['m4'], 'cursor2'));
    await store.loadMore(ROOM, 'Media');
    // The second page repeated m4 and had nothing new, so nothing is appended.
    expect(store.view(ROOM, 'Media').items.map((i) => i.event_id)).toEqual(['m5', 'm4']);

    // A picture sent while the panel is open: the first page now starts with it.
    invoke.mockResolvedValueOnce(page(['m7', 'm6', 'm5', 'm4'], 'cursor3'));
    await store.refresh(ROOM, 'Media');

    expect(store.view(ROOM, 'Media').items.map((i) => i.event_id)).toEqual(['m7', 'm6', 'm5', 'm4']);
  });

  it('leaves the list alone when a refresh fails', async () => {
    invoke.mockResolvedValueOnce(page(['m1'], null));
    await store.load(ROOM, 'Media');
    invoke.mockRejectedValueOnce(new Error('offline'));
    await store.refresh(ROOM, 'Media');
    expect(store.view(ROOM, 'Media').items.map((i) => i.event_id)).toEqual(['m1']);
    expect(store.view(ROOM, 'Media').error).toBeNull();
  });

  it('treats a refresh before the first read as the first read', async () => {
    invoke.mockResolvedValueOnce(page(['m1'], null));
    await store.refresh(ROOM, 'Files');
    expect(store.view(ROOM, 'Files').items.map((i) => i.event_id)).toEqual(['m1']);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('records a failure and retries on the next load', async () => {
    invoke.mockRejectedValueOnce(new Error('boom'));
    await store.load(ROOM, 'Links');

    expect(store.view(ROOM, 'Links').error).toBe('boom');
    expect(store.view(ROOM, 'Links').loading).toBe(false);

    invoke.mockResolvedValueOnce(page(['l1'], null));
    await store.load(ROOM, 'Links');

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(store.view(ROOM, 'Links').error).toBeNull();
    expect(store.view(ROOM, 'Links').items).toHaveLength(1);
  });
});
