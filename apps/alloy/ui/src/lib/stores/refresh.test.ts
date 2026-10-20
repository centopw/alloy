import { beforeEach, describe, expect, it, vi } from 'vitest';

const calls = vi.hoisted(() => ({
  order: [] as string[],
  refreshSync: vi.fn(),
  roomsStart: vi.fn(),
  open: vi.fn(),
  timeline: { roomId: '!room:hs' as string | null, replyTo: { id: 'reply' } as unknown, editing: null as unknown },
}));

vi.mock('../ipc/index.js', () => ({ client: { refreshSync: calls.refreshSync } }));
vi.mock('./rooms.svelte.js', () => ({ roomsStore: { restart: calls.roomsStart } }));
vi.mock('./timeline.svelte.js', () => ({
  timelineStore: Object.assign(calls.timeline, { open: calls.open }),
}));
vi.mock('./tags.svelte.js', () => ({ tags: { load: vi.fn() } }));
vi.mock('./prefs.svelte.js', () => ({ prefs: { load: vi.fn() } }));
vi.mock('./notificationSettings.svelte.js', () => ({ notificationSettings: { load: vi.fn() } }));

import { refreshStore } from './refresh.svelte';

describe('refreshStore', () => {
  beforeEach(() => {
    calls.order.length = 0;
    calls.refreshSync.mockReset().mockImplementation(async () => {
      calls.order.push('sync');
      return { type: 'SyncRefreshed' };
    });
    calls.roomsStart.mockReset().mockImplementation(async () => void calls.order.push('rooms'));
    calls.open.mockReset().mockImplementation(async () => {
      calls.order.push('open');
      calls.timeline.replyTo = null;
    });
    calls.timeline.roomId = '!room:hs';
    calls.timeline.replyTo = { id: 'reply' };
    refreshStore.error = null;
  });

  it('restarts sync first, then rebuilds the room list and reopens the same chat keeping its reply', async () => {
    await refreshStore.run();
    expect(calls.order).toEqual(['sync', 'rooms', 'open']);
    expect(calls.open).toHaveBeenCalledExactlyOnceWith('!room:hs');
    expect(calls.timeline.replyTo).toEqual({ id: 'reply' });
    expect(refreshStore.busy).toBe(false);
  });

  it('does not reopen a chat when none is open', async () => {
    calls.timeline.roomId = null;
    await refreshStore.run();
    expect(calls.open).not.toHaveBeenCalled();
  });

  it('ignores a second refresh while one is running', async () => {
    const first = refreshStore.run();
    await refreshStore.run();
    await first;
    expect(calls.refreshSync).toHaveBeenCalledTimes(1);
  });

  it('reports a failure and becomes usable again', async () => {
    calls.refreshSync.mockRejectedValueOnce({ code: 'auth_failed', message: 'not logged in' });
    await refreshStore.run();
    expect(refreshStore.error).toBe('not logged in');
    expect(refreshStore.busy).toBe(false);
    await refreshStore.run();
    expect(refreshStore.error).toBeNull();
  });
});
