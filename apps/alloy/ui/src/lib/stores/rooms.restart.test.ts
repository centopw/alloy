import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('../ipc/index.js', () => ({
  client: {
    subscribe: () => () => {},
    invoke: ipc.invoke,
  },
  applyDiff: vi.fn(),
}));
vi.mock('./prefs.svelte.js', () => ({
  prefs: { roomSort: 'activity', roomListPeople: true },
  registerFlushHandler: () => () => {},
}));
vi.mock('./drafts.svelte.js', () => ({ draftsStore: { previewOf: () => null } }));

import { roomsStore } from './rooms.svelte';

beforeEach(() => {
  ipc.invoke.mockReset().mockImplementation(async (cmd: { action: string }) => {
    if (cmd.action === 'SubscribeRoomList') {
      return { type: 'RoomListSubscribed', payload: { subscription_id: 'sub-1' } };
    }
    if (cmd.action === 'ListAccounts') return { type: 'AccountsList', payload: { accounts: [] } };
    return { type: 'Unsubscribed' };
  });
});

describe('roomsStore.restart', () => {
  it('rebuilds the subscription but keeps the open room selected', async () => {
    await roomsStore.start();
    roomsStore.select('!open:hs');
    expect(roomsStore.activeRoomId).toBe('!open:hs');

    await roomsStore.restart();

    expect(ipc.invoke.mock.calls.filter(([c]) => c.action === 'SubscribeRoomList')).toHaveLength(2);
    expect(roomsStore.activeRoomId).toBe('!open:hs');
  });

  it('a plain start still forgets the selection, as sign-out relies on', async () => {
    roomsStore.select('!open:hs');
    await roomsStore.start();
    expect(roomsStore.activeRoomId).toBeNull();
  });
});
