import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RoomInfoStore } from './roomInfo.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

const ROOM = '!general:alloy.chat';
const UNIT = { type: 'Unit' };

function details(name: string | null) {
  return {
    room_id: ROOM,
    name,
    topic: 'Welcome',
    canonical_alias: null,
    avatar_url: null,
    is_direct: false,
    encrypted: false,
    joined_members: 2,
    invited_members: 0,
    can_set_name: true,
    can_set_topic: true,
    can_set_avatar: true,
  };
}

const detailsRes = (name: string | null) => ({
  type: 'RoomDetailsRetrieved',
  payload: { details: details(name) },
});
const noteRes = (note: string) => ({ type: 'RoomNote', payload: { room_id: ROOM, note } });

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

let store: RoomInfoStore;

beforeEach(() => {
  invoke.mockReset();
  store = new RoomInfoStore();
});

describe('RoomInfoStore', () => {
  it('loads details and note for a room', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes('remember'));
    await store.load(ROOM);

    expect(store.details(ROOM)?.name).toBe('General');
    expect(store.note(ROOM)).toBe('remember');
    expect(store.error(ROOM)).toBeNull();
  });

  it('returns empty values for an unknown room', () => {
    expect(store.details(ROOM)).toBeNull();
    expect(store.note(ROOM)).toBe('');
    expect(store.saving(ROOM)).toBe(false);
    expect(store.error(ROOM)).toBeNull();
  });

  it('serves a loaded room from cache and shares an in-flight load', async () => {
    const d = deferred<unknown>();
    const n = deferred<unknown>();
    invoke.mockReturnValueOnce(d.promise).mockReturnValueOnce(n.promise);

    const a = store.load(ROOM);
    const b = store.load(ROOM);
    expect(invoke).toHaveBeenCalledTimes(2);

    d.resolve(detailsRes('General'));
    n.resolve(noteRes(''));
    await Promise.all([a, b]);

    await store.load(ROOM);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('renames through SetRoomName, expects Unit, then re-reads details', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    await store.load(ROOM);

    invoke
      .mockResolvedValueOnce(UNIT)
      .mockResolvedValueOnce(detailsRes('Lounge'))
      .mockResolvedValueOnce(noteRes(''));
    await store.rename(ROOM, 'Lounge');

    expect(invoke).toHaveBeenNthCalledWith(3, {
      action: 'SetRoomName',
      params: { room_id: ROOM, name: 'Lounge' },
    });
    expect(store.details(ROOM)?.name).toBe('Lounge');
    expect(store.saving(ROOM)).toBe(false);
  });

  it('sends a null file_path to remove the avatar', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    await store.load(ROOM);

    invoke
      .mockResolvedValueOnce(UNIT)
      .mockResolvedValueOnce(detailsRes('General'))
      .mockResolvedValueOnce(noteRes(''));
    await store.setAvatar(ROOM, null);

    expect(invoke).toHaveBeenNthCalledWith(3, {
      action: 'SetRoomAvatar',
      params: { room_id: ROOM, file_path: null },
    });
  });

  it('saves the note through SetRoomNote and shows the re-read note', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    await store.load(ROOM);

    invoke
      .mockResolvedValueOnce(UNIT)
      .mockResolvedValueOnce(detailsRes('General'))
      .mockResolvedValueOnce(noteRes('buy milk'));
    await store.saveNote(ROOM, 'buy milk');

    expect(invoke).toHaveBeenNthCalledWith(3, {
      action: 'SetRoomNote',
      params: { room_id: ROOM, note: 'buy milk' },
    });
    expect(store.note(ROOM)).toBe('buy milk');
  });

  it('reports saving while a write is in flight', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    await store.load(ROOM);

    const write = deferred<unknown>();
    invoke.mockReturnValueOnce(write.promise);
    const pending = store.setTopic(ROOM, 'New topic');
    expect(store.saving(ROOM)).toBe(true);

    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    write.resolve(UNIT);
    await pending;
    expect(store.saving(ROOM)).toBe(false);
  });

  it('stores a rejected write as the room error without throwing', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    await store.load(ROOM);

    invoke.mockRejectedValueOnce(new Error('forbidden'));
    await expect(store.setTopic(ROOM, 'x')).resolves.toBeUndefined();

    expect(store.error(ROOM)).toBe('forbidden');
    expect(store.saving(ROOM)).toBe(false);
    expect(store.details(ROOM)?.name).toBe('General');
  });

  it('treats a reply that is not Unit as an error', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    await store.load(ROOM);

    invoke.mockResolvedValueOnce(noteRes('unexpected'));
    await store.rename(ROOM, 'Lounge');

    expect(store.error(ROOM)).toBe('Expected Unit, got RoomNote');
  });

  it('records a load failure and retries on the next load', async () => {
    invoke.mockRejectedValue(new Error('offline'));
    await store.load(ROOM);
    expect(store.error(ROOM)).toBe('offline');
    expect(store.details(ROOM)).toBeNull();

    invoke.mockReset();
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(''));
    await store.load(ROOM);

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(store.error(ROOM)).toBeNull();
    expect(store.details(ROOM)?.name).toBe('General');
  });
});
