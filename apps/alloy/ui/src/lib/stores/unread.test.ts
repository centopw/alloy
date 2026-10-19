import { describe, expect, it } from 'vitest';
import type { RoomSummary } from '../generated/index.js';
import { loadReadMarks, markFor, saveReadMarks, unreadCount } from './unread.js';

const room = (count: number, ts = 100): RoomSummary =>
  ({
    id: '!r:x',
    unread_notifications: { notification_count: count, highlight_count: 0 },
    latest_event: { sender: '@bob:x', timestamp_millis: ts, body: 'hi' },
  }) as unknown as RoomSummary;

describe('unreadCount', () => {
  it('shows the server count when the room was never read here', () => {
    expect(unreadCount(room(6), undefined)).toBe(6);
  });

  it('shows 0 for a stale count right after the room was read', () => {
    const mark = markFor(room(6));
    expect(unreadCount(room(6), mark)).toBe(0);
  });

  it('shows only what arrived after the read when the stale count keeps growing', () => {
    const mark = markFor(room(6, 100));
    expect(unreadCount(room(7, 200), mark)).toBe(1);
    expect(unreadCount(room(9, 300), mark)).toBe(3);
  });

  it('shows the recount when the server reset its counter and a new message arrived', () => {
    const mark = markFor(room(6, 100));
    expect(unreadCount(room(1, 200), mark)).toBe(1);
  });

  it('stays 0 when the server reset its counter without a new message', () => {
    const mark = markFor(room(6, 100));
    expect(unreadCount(room(0, 100), mark)).toBe(0);
    expect(unreadCount(room(2, 100), mark)).toBe(0);
  });
});

describe('read marks storage', () => {
  it('round-trips marks and ignores corrupt data', () => {
    const store = new Map<string, string>();
    const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
    const marks = { '!a:x': markFor(room(6, 100)) };
    saveReadMarks(storage, marks);
    expect(loadReadMarks(storage)).toEqual(marks);
    store.set('alloy.readMarks', '{not json');
    expect(loadReadMarks(storage)).toEqual({});
    store.set('alloy.readMarks', JSON.stringify({ '!b:x': { count: 'x' }, '!c:x': { count: 2, latest: '1:@a:x' } }));
    expect(loadReadMarks(storage)).toEqual({ '!c:x': { count: 2, latest: '1:@a:x' } });
  });

  it('keeps a room read after a reload that recomputes the same or a lower count', () => {
    const mark = markFor(room(6, 100));
    expect(unreadCount(room(6, 100), mark)).toBe(0);
    expect(unreadCount(room(3, 100), mark)).toBe(0);
  });
});
