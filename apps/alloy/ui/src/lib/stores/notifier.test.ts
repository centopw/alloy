import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import type { RoomSummary } from '$lib/generated';

vi.mock('../ipc/index.js', () => ({
  client: { invoke: vi.fn() },
}));

import {
  COALESCE_MS,
  Notifier,
  shouldNotify,
  snapshotRooms,
  type NotifierDeps,
  type NotifierSettings,
  type NotifyState,
  type RoomSnapshot,
} from './notifier.svelte';

const ALL_ON: NotifierSettings = { desktop: true, sound: true, preview: true, onlyWhenUnfocused: true };

function state(overrides: Partial<NotifyState> = {}): NotifyState {
  return {
    newNotifications: 1,
    muted: false,
    viewingRoom: false,
    windowFocused: false,
    settings: ALL_ON,
    ...overrides,
  };
}

describe('shouldNotify', () => {
  it('notifies for new activity when the window is unfocused', () => {
    expect(shouldNotify(state())).toBe(true);
  });

  it('never notifies when desktop notifications are off', () => {
    expect(shouldNotify(state({ settings: { ...ALL_ON, desktop: false } }))).toBe(false);
  });

  it('never notifies for a muted chat', () => {
    expect(shouldNotify(state({ muted: true }))).toBe(false);
  });

  it('notifies nothing when no new notifications arrived', () => {
    expect(shouldNotify(state({ newNotifications: 0 }))).toBe(false);
  });

  it('stays silent for the viewed chat while the window is focused', () => {
    expect(shouldNotify(state({ windowFocused: true, viewingRoom: true }))).toBe(false);
  });

  it('notifies while focused only when unfocused-only is off', () => {
    const focusedAlways = { ...ALL_ON, onlyWhenUnfocused: false };
    expect(shouldNotify(state({ windowFocused: true, settings: focusedAlways }))).toBe(true);
    expect(shouldNotify(state({ windowFocused: true, settings: ALL_ON }))).toBe(false);
  });
});

function summary(overrides: Partial<RoomSummary> = {}): RoomSummary {
  return {
    id: 'chat-1',
    account_id: 'account-1',
    network: 'telegram',
    name: 'Ada',
    avatar: null,
    kind: 'direct',
    is_direct: true,
    unread_notifications: { highlight_count: 0, notification_count: 2 },
    latest_event: { sender: 'u1', timestamp_millis: 1_700_000_000_000, body: 'hello' },
    pinned: false,
    archived: false,
    muted_until: null,
    tags: [],
    ...overrides,
  };
}

describe('snapshotRooms', () => {
  const NOW = 1_700_000_000_000;

  it('reads counts, preview and title from the summary', () => {
    const [room] = snapshotRooms([summary()], NOW);
    expect(room).toEqual({
      id: 'chat-1',
      title: 'Ada',
      preview: 'hello',
      notifications: 2,
      highlights: 0,
      muted: false,
    });
  });

  it('treats a running mute, including a permanent one, as muted and an expired mute as open', () => {
    const rooms = [
      summary({ id: 'a', muted_until: NOW + 60_000 }),
      summary({ id: 'b', muted_until: NOW - 1 }),
      summary({ id: 'c', muted_until: 18_446_744_073_709_551_615 }),
      summary({ id: 'd', muted_until: null }),
    ];
    expect(snapshotRooms(rooms, NOW).map((r) => r.muted)).toEqual([true, false, true, false]);
  });

  it('has no preview for a chat whose latest message is unknown', () => {
    const [room] = snapshotRooms([summary({ latest_event: null })], NOW);
    expect(room.preview).toBeNull();
  });
});

function room(id: string, notifications: number, preview: string | null = 'hello', muted = false): RoomSnapshot {
  return { id, title: id, preview, notifications, highlights: 0, muted };
}

let clock = 0;
let deps: NotifierDeps & {
  show: Mock;
};

beforeEach(() => {
  vi.useFakeTimers();
  clock = 1_000_000;
  deps = {
    now: () => clock,
    windowFocused: () => false,
    activeRoomId: () => null,
    show: vi.fn().mockResolvedValue(undefined),
    settings: () => ALL_ON,
  };
});

afterEach(() => {
  vi.useRealTimers();
});

describe('Notifier', () => {
  it('records the first sighting of a chat without notifying for existing unread', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 5)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).not.toHaveBeenCalled();
  });

  it('notifies once a known chat gains notifications', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 2)]);
    notifier.observe([room('a', 3, 'new message')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledOnce();
    expect(deps.show).toHaveBeenCalledWith({ title: 'a', body: 'new message', silent: false, roomId: 'a' });
  });

  it('hides the preview and silences the sound when those preferences are off', async () => {
    deps.settings = () => ({ ...ALL_ON, preview: false, sound: false });
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 0)]);
    notifier.observe([room('a', 1, 'secret')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledWith({ title: 'a', body: 'New message', silent: true, roomId: 'a' });
  });

  it('does not notify for a muted chat', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 0)]);
    notifier.observe([room('a', 1, 'hello', true)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).not.toHaveBeenCalled();
  });

  it('records a failure to show a notification', async () => {
    deps.show.mockRejectedValue(new Error('no permission'));
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 0)]);
    notifier.observe([room('a', 1)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(notifier.error).toBe('no permission');
  });

  it('coalesces bursts per chat and delivers the latest notice after the window', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 0)]);
    notifier.observe([room('a', 1, 'first')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledTimes(1);

    clock += 500;
    notifier.observe([room('a', 2, 'second')]);
    notifier.observe([room('a', 3, 'third')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledTimes(1);

    clock += COALESCE_MS;
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(deps.show).toHaveBeenCalledTimes(2);
    expect(deps.show).toHaveBeenLastCalledWith({ title: 'a', body: 'third', silent: false, roomId: 'a' });
  });

  it('keeps coalescing separate per chat', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 0), room('b', 0)]);
    notifier.observe([room('a', 1), room('b', 1)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledTimes(2);
  });

  it('drops queued notices on stop', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('a', 0)]);
    notifier.observe([room('a', 1)]);
    notifier.observe([room('a', 2)]);
    await vi.advanceTimersByTimeAsync(0);
    notifier.stop();
    await vi.advanceTimersByTimeAsync(COALESCE_MS * 2);
    expect(deps.show).toHaveBeenCalledTimes(1);
  });
});
