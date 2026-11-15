import { describe, expect, it } from 'vitest';
import type { RoomSummary } from '$lib/generated';
import { forumTopicCount, forumTotals, topicsOf } from './forumTopics';

function room(id: string, patch: Partial<RoomSummary> = {}): RoomSummary {
  return {
    id,
    account_id: 'acc',
    network: 'telegram',
    name: id,
    avatar: null,
    kind: 'group',
    is_direct: false,
    unread_notifications: { highlight_count: 0, notification_count: 0 },
    latest_event: null,
    pinned: false,
    pin_order: null,
    archived: false,
    muted_until: null,
    marked_unread: false,
    tags: [],
    draft: null,
    ...patch,
  } as RoomSummary;
}

const at = (millis: number) => ({ timestamp_millis: millis }) as RoomSummary['latest_event'];

describe('topicsOf', () => {
  it('returns only the topics of that forum, newest activity first', () => {
    const rooms = [
      room('dm'),
      room('forum', { is_forum: true }),
      room('old', { parent_id: 'forum', latest_event: at(100) }),
      room('new', { parent_id: 'forum', latest_event: at(900) }),
      room('other', { parent_id: 'elsewhere' }),
    ];
    expect(topicsOf(rooms, 'forum').map((r) => r.id)).toEqual(['new', 'old']);
  });

  it('breaks a tie in activity by name, so the forum opens the same topic every time', () => {
    const rooms = [
      room('forum', { is_forum: true }),
      room('b', { parent_id: 'forum', latest_event: at(100) }),
      room('a', { parent_id: 'forum', latest_event: at(100) }),
      room('none', { parent_id: 'forum' }),
    ];
    expect(topicsOf(rooms, 'forum').map((r) => r.id)).toEqual(['a', 'b', 'none']);
  });

  it('keeps a topic that has no messages, after the active ones', () => {
    const rooms = [
      room('forum', { is_forum: true }),
      room('quiet', { parent_id: 'forum' }),
      room('busy', { parent_id: 'forum', latest_event: at(5) }),
    ];
    expect(topicsOf(rooms, 'forum').map((r) => r.id)).toEqual(['busy', 'quiet']);
  });

  it('is empty for a forum without topics', () => {
    expect(topicsOf([room('forum', { is_forum: true })], 'forum')).toEqual([]);
  });
});

describe('forumTopicCount', () => {
  it('counts the topics of one forum only', () => {
    const rooms = [
      room('forum', { is_forum: true }),
      room('t1', { parent_id: 'forum' }),
      room('t2', { parent_id: 'forum' }),
      room('t3', { parent_id: 'elsewhere' }),
      room('dm'),
    ];
    expect(forumTopicCount(rooms, 'forum')).toBe(2);
  });

  it('is zero for a forum without topics', () => {
    expect(forumTopicCount([room('forum', { is_forum: true })], 'forum')).toBe(0);
  });
});

describe('forumTotals', () => {
  const counts = (r: RoomSummary) => r.unread_notifications.notification_count;

  it('sums the unread of the forum and its topics', () => {
    const forum = room('forum', {
      is_forum: true,
      unread_notifications: { highlight_count: 1, notification_count: 2 },
    });
    const topics = [
      room('t1', { unread_notifications: { highlight_count: 0, notification_count: 3 } }),
      room('t2', { unread_notifications: { highlight_count: 2, notification_count: 0 } }),
    ];
    expect(forumTotals(forum, topics, counts)).toEqual({ notifications: 5, highlights: 3 });
  });

  it('uses the caller-supplied unread count, so read marks apply to topics too', () => {
    const forum = room('forum', { unread_notifications: { highlight_count: 0, notification_count: 4 } });
    const topics = [room('t1', { unread_notifications: { highlight_count: 0, notification_count: 4 } })];
    const read = (r: RoomSummary) => (r.id === 't1' ? 0 : counts(r));
    expect(forumTotals(forum, topics, read)).toEqual({ notifications: 4, highlights: 0 });
  });

  it('is the forum alone when it has no topics', () => {
    const forum = room('forum', { unread_notifications: { highlight_count: 0, notification_count: 7 } });
    expect(forumTotals(forum, [], counts)).toEqual({ notifications: 7, highlights: 0 });
  });
});
