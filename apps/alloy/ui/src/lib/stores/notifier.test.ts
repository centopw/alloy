import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';

vi.mock('../ipc/index.js', () => ({
  client: { invoke: vi.fn() },
}));

import {
  COALESCE_MS,
  Notifier,
  shouldNotify,
  type NotifierDeps,
  type NotifierSettings,
  type NotifyState,
  type RoomSnapshot,
} from './notifier.svelte';

const ALL_ON: NotifierSettings = { desktop: true, sound: true, preview: true, onlyWhenUnfocused: true };

function state(overrides: Partial<NotifyState> = {}): NotifyState {
  return {
    level: 'All',
    newNotifications: 1,
    newHighlights: 0,
    viewingRoom: false,
    windowFocused: false,
    settings: ALL_ON,
    ...overrides,
  };
}

describe('shouldNotify', () => {
  it('notifies for new activity in an All room when the window is unfocused', () => {
    expect(shouldNotify(state())).toBe(true);
  });

  it('never notifies when desktop notifications are off', () => {
    expect(shouldNotify(state({ settings: { ...ALL_ON, desktop: false } }))).toBe(false);
  });

  it('never notifies for a muted room', () => {
    expect(shouldNotify(state({ level: 'Mute' }))).toBe(false);
  });

  it('notifies a Mentions room only for new highlights', () => {
    expect(shouldNotify(state({ level: 'Mentions', newHighlights: 0 }))).toBe(false);
    expect(shouldNotify(state({ level: 'Mentions', newHighlights: 1 }))).toBe(true);
  });

  it('notifies nothing when no new notifications arrived', () => {
    expect(shouldNotify(state({ newNotifications: 0 }))).toBe(false);
  });

  it('stays silent for the viewed room while the window is focused', () => {
    expect(shouldNotify(state({ windowFocused: true, viewingRoom: true }))).toBe(false);
  });

  it('notifies while focused only when unfocused-only is off', () => {
    const focusedAlways = { ...ALL_ON, onlyWhenUnfocused: false };
    expect(shouldNotify(state({ windowFocused: true, settings: focusedAlways }))).toBe(true);
    expect(shouldNotify(state({ windowFocused: true, settings: ALL_ON }))).toBe(false);
  });
});

function room(id: string, notifications: number, highlights = 0, preview: string | null = 'hello'): RoomSnapshot {
  return { id, title: id, preview, notifications, highlights };
}

let clock = 0;
let deps: NotifierDeps & {
  show: Mock;
  levelOf: Mock;
};

beforeEach(() => {
  vi.useFakeTimers();
  clock = 1_000_000;
  deps = {
    now: () => clock,
    windowFocused: () => false,
    activeRoomId: () => null,
    levelOf: vi.fn().mockResolvedValue('All'),
    show: vi.fn().mockResolvedValue(undefined),
    settings: () => ALL_ON,
  };
});

afterEach(() => {
  vi.useRealTimers();
});

describe('Notifier', () => {
  it('records the first sighting of a room without notifying for existing unread', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 5)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).not.toHaveBeenCalled();
  });

  it('notifies once a known room gains notifications', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 2)]);
    notifier.observe([room('!a', 3, 0, 'new message')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledOnce();
    expect(deps.show).toHaveBeenCalledWith({ title: '!a', body: 'new message', silent: false, roomId: '!a' });
  });

  it('hides the preview when previews are disabled', async () => {
    deps.settings = () => ({ ...ALL_ON, preview: false, sound: false });
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 0)]);
    notifier.observe([room('!a', 1, 0, 'secret')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledWith({ title: '!a', body: 'New message', silent: true, roomId: '!a' });
  });

  it('does not notify for a room whose engine level is Mute', async () => {
    deps.levelOf.mockResolvedValue('Mute');
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 0)]);
    notifier.observe([room('!a', 1)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).not.toHaveBeenCalled();
  });

  it('records a level read failure and shows nothing', async () => {
    deps.levelOf.mockRejectedValue(new Error('no level'));
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 0)]);
    notifier.observe([room('!a', 1)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).not.toHaveBeenCalled();
    expect(notifier.error).toBe('no level');
  });

  it('coalesces bursts per room and delivers the latest notice after the window', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 0)]);
    notifier.observe([room('!a', 1, 0, 'first')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledTimes(1);

    clock += 500;
    notifier.observe([room('!a', 2, 0, 'second')]);
    notifier.observe([room('!a', 3, 0, 'third')]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledTimes(1);

    clock += COALESCE_MS;
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(deps.show).toHaveBeenCalledTimes(2);
    expect(deps.show).toHaveBeenLastCalledWith({ title: '!a', body: 'third', silent: false, roomId: '!a' });
  });

  it('keeps coalescing separate per room', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 0), room('!b', 0)]);
    notifier.observe([room('!a', 1), room('!b', 1)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.show).toHaveBeenCalledTimes(2);
  });

  it('drops queued notices on stop', async () => {
    const notifier = new Notifier(deps);
    notifier.observe([room('!a', 0)]);
    notifier.observe([room('!a', 1)]);
    notifier.observe([room('!a', 2)]);
    await vi.advanceTimersByTimeAsync(0);
    notifier.stop();
    await vi.advanceTimersByTimeAsync(COALESCE_MS * 2);
    expect(deps.show).toHaveBeenCalledTimes(1);
  });
});
