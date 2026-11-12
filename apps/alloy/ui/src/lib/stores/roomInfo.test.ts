import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RoomInfoStore } from './roomInfo.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));
vi.mock('./prefs.svelte.js', () => ({ registerFlushHandler: vi.fn() }));

const ROOM = 'chat-1';
const NOTE_SET = { type: 'RoomNoteSet' };

function details(name: string | null) {
  return {
    room_id: ROOM,
    account_id: 'acc-1',
    network: 'telegram',
    name,
    avatar: null,
    kind: 'group',
    is_direct: false,
    member_count: 2,
    note: null,
    capabilities: {
      edit: true,
      edit_window_secs: null,
      delete_for_everyone: true,
      reactions: true,
      max_reactions_per_user: null,
      replies: true,
      threads: false,
      typing: true,
      read_receipts: true,
      delivered_receipts: false,
      voice: true,
      polls: false,
      location: false,
      max_upload_bytes: null,
      formatted_text: false,
    },
  };
}

const detailsRes = (name: string | null) => ({
  type: 'RoomDetailsRetrieved',
  payload: { details: details(name) },
});
const noteRes = (note: string | null) => ({ type: 'RoomNote', payload: { note } });

function deferred<T>() {
  const { promise, resolve } = Promise.withResolvers<T>();
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
    n.resolve(noteRes(null));
    await Promise.all([a, b]);

    await store.load(ROOM);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('saves the note through SetRoomNote and shows the re-read note', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(null));
    await store.load(ROOM);

    invoke
      .mockResolvedValueOnce(NOTE_SET)
      .mockResolvedValueOnce(detailsRes('General'))
      .mockResolvedValueOnce(noteRes('kept'));
    await store.saveNote(ROOM, 'kept');

    expect(invoke).toHaveBeenNthCalledWith(3, {
      action: 'SetRoomNote',
      params: { room_id: ROOM, note: 'kept' },
    });
    expect(store.note(ROOM)).toBe('kept');
    expect(store.saving(ROOM)).toBe(false);
  });

  it('stores a write failure instead of rejecting', async () => {
    invoke.mockResolvedValueOnce(detailsRes('General')).mockResolvedValueOnce(noteRes(null));
    await store.load(ROOM);

    invoke.mockRejectedValueOnce({ code: 'internal_error', message: 'write failed' });
    await store.saveNote(ROOM, 'x');

    expect(store.error(ROOM)).toBe('write failed');
    expect(store.saving(ROOM)).toBe(false);
  });

  it('drops a response that belongs to an account that was signed out meanwhile', async () => {
    const d = deferred<unknown>();
    const n = deferred<unknown>();
    invoke.mockReturnValueOnce(d.promise).mockReturnValueOnce(n.promise);

    const loading = store.load(ROOM);
    store.reset();
    d.resolve(detailsRes('Old account'));
    n.resolve(noteRes('old note'));
    await loading;

    expect(store.details(ROOM)).toBeNull();
    expect(store.note(ROOM)).toBe('');
  });
});
