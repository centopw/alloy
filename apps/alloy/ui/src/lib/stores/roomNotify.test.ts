import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RoomNotifyStore } from './roomNotify.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

const ROOM = '!general:alloy.chat';
const levelRes = (level: string, is_default: boolean) => ({
  type: 'RoomNotificationLevel',
  payload: { room_id: ROOM, level, is_default },
});

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

let store: RoomNotifyStore;

beforeEach(() => {
  invoke.mockReset();
  store = new RoomNotifyStore();
});

describe('RoomNotifyStore', () => {
  it('reports unknown until the room was read', () => {
    expect(store.levelOf(ROOM)).toBeNull();
    expect(store.isDefault(ROOM)).toBeNull();
    expect(store.isMuted(ROOM)).toBe(false);
  });

  it('loads the effective level and whether it follows the default', async () => {
    invoke.mockResolvedValueOnce(levelRes('Mute', false));
    await store.load(ROOM);
    expect(invoke).toHaveBeenCalledWith({ action: 'GetRoomNotificationLevel', params: { room_id: ROOM } });
    expect(store.levelOf(ROOM)).toBe('Mute');
    expect(store.isDefault(ROOM)).toBe(false);
    expect(store.isMuted(ROOM)).toBe(true);
  });

  it('reads a room once and shares a request in flight', async () => {
    const gate = deferred<unknown>();
    invoke.mockReturnValueOnce(gate.promise);
    const first = store.load(ROOM);
    const second = store.load(ROOM);
    gate.resolve(levelRes('All', true));
    await Promise.all([first, second]);
    await store.load(ROOM);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(store.levelOf(ROOM)).toBe('All');
    expect(store.isDefault(ROOM)).toBe(true);
  });

  it('stores a failed read without rejecting', async () => {
    invoke.mockRejectedValueOnce(new Error('offline'));
    await expect(store.load(ROOM)).resolves.toBeUndefined();
    expect(store.levelOf(ROOM)).toBeNull();
    expect(store.error(ROOM)).toBe('offline');
  });

  it('sets an override and re-reads the room', async () => {
    invoke
      .mockResolvedValueOnce({ type: 'Unit' })
      .mockResolvedValueOnce(levelRes('Mentions', false));
    await store.set(ROOM, 'Mentions');
    expect(invoke).toHaveBeenNthCalledWith(1, {
      action: 'SetRoomNotificationLevel',
      params: { room_id: ROOM, level: 'Mentions' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, { action: 'GetRoomNotificationLevel', params: { room_id: ROOM } });
    expect(store.levelOf(ROOM)).toBe('Mentions');
    expect(store.isDefault(ROOM)).toBe(false);
    expect(store.saving(ROOM)).toBe(false);
  });

  it('restores the account default with null', async () => {
    invoke
      .mockResolvedValueOnce(levelRes('Mute', false))
      .mockResolvedValueOnce({ type: 'Unit' })
      .mockResolvedValueOnce(levelRes('All', true));
    await store.load(ROOM);
    await store.set(ROOM, null);
    expect(invoke).toHaveBeenNthCalledWith(2, {
      action: 'SetRoomNotificationLevel',
      params: { room_id: ROOM, level: null },
    });
    expect(store.isDefault(ROOM)).toBe(true);
    expect(store.levelOf(ROOM)).toBe('All');
  });

  it('keeps the previous level and stores the error when a write fails', async () => {
    invoke
      .mockResolvedValueOnce(levelRes('All', true))
      .mockRejectedValueOnce(new Error('denied'));
    await store.load(ROOM);
    await store.set(ROOM, 'Mute');
    expect(store.levelOf(ROOM)).toBe('All');
    expect(store.error(ROOM)).toBe('denied');
    expect(store.saving(ROOM)).toBe(false);
  });

  it('clear drops every cached level so the next read goes to the engine', async () => {
    invoke.mockResolvedValueOnce(levelRes('Mute', false)).mockResolvedValueOnce(levelRes('All', true));
    await store.load(ROOM);
    store.clear();
    expect(store.levelOf(ROOM)).toBeNull();
    await store.load(ROOM);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(store.levelOf(ROOM)).toBe('All');
  });
});
