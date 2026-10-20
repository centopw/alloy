import { describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  subscribeRoomList: vi.fn(),
  unsubscribe: vi.fn(),
}));

vi.mock('../ipc/index.js', () => ({
  client: {
    subscribe: () => () => {},
    subscribeRoomList: ipc.subscribeRoomList,
    unsubscribe: ipc.unsubscribe,
  },
  applyDiff: vi.fn(),
}));

import { roomsStore } from './rooms.svelte';

describe('roomsStore.restart', () => {
  it('rebuilds the subscription but keeps the open room selected', async () => {
    ipc.subscribeRoomList.mockResolvedValue({ type: 'RoomListSubscribed', payload: { subscription_id: 'sub-1' } });
    ipc.unsubscribe.mockResolvedValue({ type: 'Unit' });

    await roomsStore.start();
    roomsStore.select('!open:hs');
    expect(roomsStore.activeRoomId).toBe('!open:hs');

    await roomsStore.restart();

    expect(ipc.subscribeRoomList).toHaveBeenCalledTimes(2);
    expect(roomsStore.activeRoomId).toBe('!open:hs');
  });

  it('a plain start still forgets the selection, as sign-out relies on', async () => {
    roomsStore.select('!open:hs');
    await roomsStore.start();
    expect(roomsStore.activeRoomId).toBeNull();
  });
});
