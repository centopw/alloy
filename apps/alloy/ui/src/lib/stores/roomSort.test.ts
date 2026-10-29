import { describe, expect, it } from 'vitest';
import type { RoomSummary, RoomTagDef } from '../generated/index.js';
import { groupRoomsByTag, sortForGroupToggle, sortRooms, type RoomSortContext } from './roomSort.js';

function room(
  id: string,
  p: { title?: string; tags?: string[]; ts?: number; unread?: number } = {},
): RoomSummary {
  return {
    id,
    name: p.title ?? id,
    avatar_url: null,
    unread_notifications: { highlight_count: 0, notification_count: p.unread ?? 0 },
    latest_event:
      p.ts === undefined ? null : { sender: '@a:x', timestamp_millis: p.ts, body: '' },
    is_direct: false,
    is_space: false,
    encrypted: false,
    membership: 'Joined',
    favourite: null,
    low_priority: null,
    tags: p.tags ?? [],
    direct_user_id: null,
  };
}

function ctxWith(tagOrder: string[] = []): RoomSortContext {
  return {
    unreadOf: (r) => r.unread_notifications.notification_count,
    titleOf: (r) => r.name ?? '',
    tagOrder,
  };
}

const ids = (rooms: RoomSummary[]) => rooms.map((r) => r.id);

describe('sortRooms', () => {
  const rooms = [
    room('a', { title: 'Bravo', ts: 100, unread: 0 }),
    room('b', { title: 'Alpha', ts: 300, unread: 2 }),
    room('c', { title: 'Charlie', ts: 200, unread: 5 }),
    room('d', { title: 'Delta', ts: 200, unread: 0 }),
  ];

  it('orders by newest activity, breaking ties by title', () => {
    expect(ids(sortRooms(rooms, 'activity', ctxWith()))).toEqual(['b', 'c', 'd', 'a']);
  });

  it('orders by title', () => {
    expect(ids(sortRooms(rooms, 'name', ctxWith()))).toEqual(['b', 'a', 'c', 'd']);
  });

  it('orders by unread count, then activity', () => {
    expect(ids(sortRooms(rooms, 'unread', ctxWith()))).toEqual(['c', 'b', 'd', 'a']);
  });

  it('orders by catalogue position of the first tag, untagged last, then activity', () => {
    const tagged = [
      room('x', { ts: 1 }),
      room('y', { tags: ['work'], ts: 1 }),
      room('z', { tags: ['home', 'work'], ts: 9 }),
      room('w', { tags: ['home'], ts: 5 }),
    ];
    expect(ids(sortRooms(tagged, 'tag', ctxWith(['home', 'work'])))).toEqual(['z', 'w', 'y', 'x']);
  });

  it('does not mutate its input', () => {
    const before = ids(rooms);
    sortRooms(rooms, 'name', ctxWith());
    expect(ids(rooms)).toEqual(before);
  });
});

describe('groupRoomsByTag', () => {
  const defs: RoomTagDef[] = [
    { id: 'work', name: 'Work', colour: 'blue' },
    { id: 'home', name: 'Home', colour: 'green' },
    { id: 'empty', name: 'Empty', colour: 'grey' },
  ];

  it('groups in catalogue order, files a multi-tag room under its first tag, drops empty groups', () => {
    const rooms = [room('h', { tags: ['home'] }), room('w', { tags: ['work', 'home'] }), room('n')];
    const groups = groupRoomsByTag(rooms, defs);
    expect(groups.map((g) => g.tag?.id ?? null)).toEqual(['work', 'home', null]);
    expect(groups.map((g) => ids(g.rooms))).toEqual([['w'], ['h'], ['n']]);
  });

  it('omits the untagged group when every room is tagged', () => {
    const groups = groupRoomsByTag([room('w', { tags: ['work'] })], defs);
    expect(groups.map((g) => g.tag?.id ?? null)).toEqual(['work']);
  });
});

describe('sortForGroupToggle', () => {
  it('turns grouping on from whatever sort is active', () => {
    expect(sortForGroupToggle('name', null)).toBe('tag');
    expect(sortForGroupToggle('unread', 'name')).toBe('tag');
  });

  it('restores the sort that was active before grouping, never a hard-coded default', () => {
    // Name -> Group -> Group off used to land in Activity and overwrite the saved preference.
    expect(sortForGroupToggle('tag', 'name')).toBe('name');
    expect(sortForGroupToggle('tag', 'unread')).toBe('unread');
  });

  it('falls back to the default only when nothing was remembered', () => {
    expect(sortForGroupToggle('tag', null)).toBe('activity');
  });
});
