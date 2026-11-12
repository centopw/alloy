import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RoomSummary } from '$lib/generated';
import type { MenuAction, MenuItem } from '$lib/stores/contextmenu.svelte';
import { copyItems, notifyItems } from './roomMenus';

const mocks = vi.hoisted(() => ({
  copyText: vi.fn(async (_text: string) => undefined),
  muteRoom: vi.fn(async (_roomId: string, _ms: number | null) => undefined),
  unmuteRoom: vi.fn(async (_roomId: string) => undefined),
  rooms: [] as RoomSummary[],
}));

vi.mock('$lib/platform/clipboard', () => ({ copyText: mocks.copyText }));
vi.mock('$lib/stores/muteTimer.svelte', () => ({
  muteRoom: mocks.muteRoom,
  unmuteRoom: mocks.unmuteRoom,
}));
vi.mock('$lib/stores/rooms.svelte', () => ({
  roomsStore: {
    get rooms() {
      return mocks.rooms;
    },
  },
  isRoomMuted: (room: Pick<RoomSummary, 'muted_until'>, now = Date.now()) =>
    room.muted_until !== null && room.muted_until > now,
}));

function room(overrides: Partial<RoomSummary> = {}): RoomSummary {
  return {
    id: 'chat-1',
    account_id: 'acc-1',
    network: 'telegram',
    name: 'Chat',
    avatar: null,
    kind: 'direct',
    is_direct: true,
    unread_notifications: { highlight_count: 0, notification_count: 0 },
    latest_event: null,
    pinned: false,
    archived: false,
    muted_until: null,
    tags: [],
    ...overrides,
  };
}

/** Menu entries that are actions (separators have no label or selection state). */
function actions(items: MenuItem[]): MenuAction[] {
  return items.filter((i): i is MenuAction => !('separator' in i));
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.rooms = [room()];
});

describe('notifyItems', () => {
  it('marks notifications on as selected when the chat is not muted', () => {
    const items = actions(notifyItems('chat-1'));
    expect(items.map((i) => i.id)).toEqual([
      'notify-unmuted',
      'mute-3600000',
      'mute-28800000',
      'mute-86400000',
      'mute-forever',
    ]);
    expect(items.filter((i) => i.selected).map((i) => i.id)).toEqual(['notify-unmuted']);
  });

  it('marks nothing selected while the chat is muted', () => {
    mocks.rooms = [room({ muted_until: Date.now() + 60_000 })];
    expect(actions(notifyItems('chat-1')).some((i) => i.selected)).toBe(false);
  });

  it('unmutes with the notifications row', () => {
    mocks.rooms = [room({ muted_until: 1 })];
    actions(notifyItems('chat-1')).find((i) => i.id === 'notify-unmuted')?.onselect?.();
    expect(mocks.unmuteRoom).toHaveBeenCalledWith('chat-1');
  });

  it('mutes for the chosen duration, and forever for the last row', () => {
    const items = actions(notifyItems('chat-1'));
    items.find((i) => i.id === 'mute-28800000')?.onselect?.();
    items.find((i) => i.id === 'mute-forever')?.onselect?.();
    expect(mocks.muteRoom).toHaveBeenNthCalledWith(1, 'chat-1', 8 * 3_600_000);
    expect(mocks.muteRoom).toHaveBeenNthCalledWith(2, 'chat-1', null);
  });
});

describe('copyItems', () => {
  it('offers the chat id, which is also the id the store and engine use', () => {
    const items = actions(copyItems(room()));
    expect(items.map((i) => i.id)).toEqual(['copy-room-id']);
    items[0].onselect?.();
    expect(mocks.copyText).toHaveBeenCalledWith('chat-1');
  });

  it('never offers a network-specific user handle', () => {
    expect(actions(copyItems(room())).some((i) => /user/i.test(i.label))).toBe(false);
  });
});
