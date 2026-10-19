import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RoomSummary } from '$lib/generated';
import type { MenuAction, MenuItem } from '$lib/stores/contextmenu.svelte';
import { copyItems, notifyItems, notifyLabel, roomPictureItems } from './roomMenus';

const mocks = vi.hoisted(() => ({
  copyText: vi.fn(async (_text: string) => undefined),
  pickFile: vi.fn(async (): Promise<string | null> => null),
  details: vi.fn((): unknown => null),
  roomInfoSaving: vi.fn(() => false),
  roomInfoError: vi.fn((): string | null => null),
  setAvatar: vi.fn(async (_roomId: string, _path: string | null) => undefined),
  isDefault: vi.fn((): boolean | null => true),
  levelOf: vi.fn((): string | null => 'All'),
  notifySaving: vi.fn(() => false),
  notifySet: vi.fn(async (_roomId: string, _level: string | null) => undefined),
  openRoomInfo: vi.fn(),
}));

vi.mock('$lib/platform/clipboard', () => ({ copyText: mocks.copyText }));
vi.mock('$lib/platform/files', () => ({ pickFile: mocks.pickFile }));
vi.mock('$lib/stores/roomInfo.svelte', () => ({
  roomInfo: {
    details: mocks.details,
    saving: mocks.roomInfoSaving,
    error: mocks.roomInfoError,
    setAvatar: mocks.setAvatar,
  },
}));
vi.mock('$lib/stores/roomNotify.svelte', () => ({
  roomNotify: {
    isDefault: mocks.isDefault,
    levelOf: mocks.levelOf,
    saving: mocks.notifySaving,
    set: mocks.notifySet,
  },
}));
vi.mock('$lib/stores/ui.svelte', () => ({ ui: { openRoomInfo: mocks.openRoomInfo } }));

function room(overrides: Partial<RoomSummary> = {}): RoomSummary {
  return { id: '!room:example.org', name: 'Room', direct_user_id: null, ...overrides } as RoomSummary;
}

/** Menu entries that are actions (separators have no label or selection state). */
function actions(items: MenuItem[]): MenuAction[] {
  return items.filter((i): i is MenuAction => !('separator' in i));
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.isDefault.mockReturnValue(true);
  mocks.levelOf.mockReturnValue('All');
  mocks.details.mockReturnValue(null);
  mocks.roomInfoError.mockReturnValue(null);
  mocks.pickFile.mockResolvedValue(null);
});

describe('notifyItems', () => {
  it('marks the account default as selected when the room follows it', () => {
    const items = actions(notifyItems('!r:x'));
    expect(items.map((i) => i.label)).toEqual([
      'Use default',
      'All messages',
      'Mentions and keywords only',
      'Mute',
    ]);
    expect(items.filter((i) => i.selected).map((i) => i.id)).toEqual(['notify-default']);
  });

  it('marks only the overridden level as selected', () => {
    mocks.isDefault.mockReturnValue(false);
    mocks.levelOf.mockReturnValue('Mentions');
    const items = actions(notifyItems('!r:x'));
    expect(items.filter((i) => i.selected).map((i) => i.id)).toEqual(['notify-Mentions']);
  });

  it('writes the chosen level, or null for the default', () => {
    const items = actions(notifyItems('!r:x'));
    items.find((i) => i.id === 'notify-Mute')?.onselect?.();
    items.find((i) => i.id === 'notify-default')?.onselect?.();
    expect(mocks.notifySet).toHaveBeenNthCalledWith(1, '!r:x', 'Mute');
    expect(mocks.notifySet).toHaveBeenNthCalledWith(2, '!r:x', null);
  });

  it('disables every choice while a write is in flight', () => {
    mocks.notifySaving.mockReturnValue(true);
    expect(actions(notifyItems('!r:x')).every((i) => i.disabled)).toBe(true);
  });
});

describe('notifyLabel', () => {
  it('names the current choice for the bell button', () => {
    mocks.isDefault.mockReturnValue(false);
    mocks.levelOf.mockReturnValue('All');
    expect(notifyLabel('!r:x')).toBe('Notifications: All messages');
  });

  it('falls back to a plain label while the level is unknown', () => {
    mocks.isDefault.mockReturnValue(null);
    expect(notifyLabel('!r:x')).toBe('Notifications');
  });
});

describe('copyItems', () => {
  it('offers only the room ID for a non-direct room', () => {
    const items = actions(copyItems(room()));
    expect(items.map((i) => i.label)).toEqual(['Copy room ID']);
  });

  it('leads with the other person\'s user ID for a direct room', () => {
    const dm = room({ direct_user_id: '@facebook_100055605049765:matrix.hce.vn' } as Partial<RoomSummary>);
    const items = actions(copyItems(dm));
    expect(items.map((i) => i.label)).toEqual(['Copy user ID', 'Copy room ID']);
    items[0].onselect?.();
    expect(mocks.copyText).toHaveBeenCalledWith('@facebook_100055605049765:matrix.hce.vn');
  });

  it('copies the room ID for the room entry', () => {
    actions(copyItems(room())).find((i) => i.id === 'copy-room-id')?.onselect?.();
    expect(mocks.copyText).toHaveBeenCalledWith('!room:example.org');
  });

  it('never offers a matrix.to link', () => {
    const dm = room({ direct_user_id: '@a:example.org' } as Partial<RoomSummary>);
    expect(actions(copyItems(dm)).some((i) => /link/i.test(i.label))).toBe(false);
  });
});

describe('roomPictureItems', () => {
  it('disables both actions without permission', () => {
    mocks.details.mockReturnValue({ can_set_avatar: false, avatar_url: 'mxc://x/y' });
    const items = actions(roomPictureItems('!r:x'));
    expect(items.map((i) => i.disabled)).toEqual([true, true]);
  });

  it('enables replace and remove for a room admin with a picture', () => {
    mocks.details.mockReturnValue({ can_set_avatar: true, avatar_url: 'mxc://x/y' });
    const items = actions(roomPictureItems('!r:x'));
    expect(items.map((i) => i.disabled)).toEqual([false, false]);
  });

  it('disables removal when there is no picture to remove', () => {
    mocks.details.mockReturnValue({ can_set_avatar: true, avatar_url: null });
    const remove = actions(roomPictureItems('!r:x')).find((i) => i.id === 'picture-remove');
    expect(remove?.disabled).toBe(true);
  });

  it('removes the picture by writing a null path', () => {
    mocks.details.mockReturnValue({ can_set_avatar: true, avatar_url: 'mxc://x/y' });
    actions(roomPictureItems('!r:x')).find((i) => i.id === 'picture-remove')?.onselect?.();
    expect(mocks.setAvatar).toHaveBeenCalledWith('!r:x', null);
  });

  it('replaces the picture with the picked file', async () => {
    mocks.details.mockReturnValue({ can_set_avatar: true, avatar_url: null });
    mocks.pickFile.mockResolvedValue('/tmp/avatar.png');
    actions(roomPictureItems('!r:x')).find((i) => i.id === 'picture-replace')?.onselect?.();
    await vi.waitFor(() => expect(mocks.setAvatar).toHaveBeenCalledWith('!r:x', '/tmp/avatar.png'));
    expect(mocks.openRoomInfo).not.toHaveBeenCalled();
  });

  it('opens room info when the write fails', async () => {
    mocks.details.mockReturnValue({ can_set_avatar: true, avatar_url: null });
    mocks.pickFile.mockResolvedValue('/tmp/avatar.png');
    mocks.roomInfoError.mockReturnValue('forbidden');
    actions(roomPictureItems('!r:x')).find((i) => i.id === 'picture-replace')?.onselect?.();
    await vi.waitFor(() => expect(mocks.openRoomInfo).toHaveBeenCalledWith('info'));
  });
});
