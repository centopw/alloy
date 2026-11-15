import { describe, it, expect, beforeEach } from 'vitest';
import { Subscription } from './subscription';
import { orderPinned, roomsStore, roomTitle, unreadChats } from './rooms.svelte';
import { timelineStore } from './timeline.svelte';
import { session } from './session.svelte';
import type { EngineEvent, RoomSummary } from '../generated/index.js';

type Diff = Extract<EngineEvent, { type: 'RoomListDiff' }>;
const ev = (id: string, _tag: string): Diff => ({
  type: 'RoomListDiff',
  payload: { subscription_id: id, diff: { type: 'Clear' } as never },
}) as Diff;

describe('Subscription', () => {
  it('buffers events until the id is known and replays only matching ones', () => {
    const seen: string[] = [];
    const sub = new Subscription<Diff>(
      (e) => e.payload.subscription_id,
      (e) => seen.push(e.payload.subscription_id)
    );
    sub.push(ev('a', '1'));
    sub.push(ev('b', '2'));
    sub.push(ev('a', '3'));
    expect(seen).toEqual([]);
    sub.attach('a');
    expect(seen).toEqual(['a', 'a']);
    sub.push(ev('b', '4'));
    sub.push(ev('a', '5'));
    expect(seen).toEqual(['a', 'a', 'a']);
  });

  it('ignores everything after close', () => {
    const seen: string[] = [];
    const sub = new Subscription<Diff>((e) => e.payload.subscription_id, (e) => seen.push('x'));
    sub.attach('a');
    sub.close();
    sub.push(ev('a', '1'));
    expect(seen).toEqual([]);
  });
});

async function until(cond: () => boolean, ms = 3000) {
  const t0 = Date.now();
  while (!cond()) {
    if (Date.now() - t0 > ms) throw new Error('condition not met in time');
    await new Promise((r) => setTimeout(r, 10));
  }
}

describe('stores against the mock engine', () => {
  beforeEach(async () => {
    await roomsStore.stop();
    await timelineStore.close();
  });

  it('session lists the connected accounts and enters the app', async () => {
    await session.boot();
    expect(session.phase).toBe('ready');
    expect(session.accounts.length).toBeGreaterThan(0);
    expect(session.accounts.map((a) => a.network)).toContain('telegram');
  });

  it('walks a Telegram login from phone number to Done and closes the overlay', async () => {
    await session.boot();
    session.openAddAccount();
    expect(session.addingAccount).toBe(true);

    await session.beginLogin('telegram');
    expect(session.login?.step.type).toBe('phone_number');
    await session.submitLogin({ type: 'phone_number', phone: '+15550100' });
    expect(session.login?.step.type).toBe('code');
    await session.submitLogin({ type: 'code', code: '12345' });
    // The mock asks for the two-step password only for some numbers; answer it if it comes.
    while (session.login?.step.type === 'password') {
      await session.submitLogin({ type: 'password', password: 'alloy-test' });
    }

    expect(session.login).toBeNull();
    expect(session.addingAccount).toBe(false);
    expect(session.phase).toBe('ready');
    expect(session.accounts.some((a) => a.network === 'telegram')).toBe(true);
  });

  it('room list streams rooms and filters by chip and query', async () => {
    await roomsStore.start();
    await until(() => roomsStore.visible.length > 0);
    const all = roomsStore.visible.length;

    roomsStore.filter = 'direct';
    expect(roomsStore.visible.every((r) => r.is_direct)).toBe(true);
    roomsStore.filter = 'rooms';
    expect(roomsStore.visible.every((r) => !r.is_direct)).toBe(true);
    roomsStore.filter = 'all';

    const first = roomsStore.visible[0];
    roomsStore.query = roomTitle(first).slice(0, 3).toUpperCase();
    expect(roomsStore.visible.some((r) => r.id === first.id)).toBe(true);
    roomsStore.query = 'zzz-no-such-room';
    expect(roomsStore.visible).toHaveLength(0);
    roomsStore.query = '';
    expect(roomsStore.visible).toHaveLength(all);
  });

  it('lists the chats of every account as one list: pinned first, then newest activity', async () => {
    await roomsStore.start();
    await until(() => roomsStore.visible.length > 0 && roomsStore.accounts.length > 1);
    const list = roomsStore.visible;
    expect(new Set(list.map((r) => r.account_id)).size).toBeGreaterThan(1);

    const firstUnpinned = list.findIndex((r) => !r.pinned);
    expect(list.slice(firstUnpinned).some((r) => r.pinned)).toBe(false);
    const times = list.slice(firstUnpinned).map((r) => r.latest_event?.timestamp_millis ?? 0);
    expect(times).toEqual([...times].sort((a, b) => b - a));
    // Not account by account: the networks interleave in activity order.
    const networks = list.slice(firstUnpinned).map((r) => r.network);
    expect(networks.filter((n, i) => i > 0 && n !== networks[i - 1]).length).toBeGreaterThan(1);
  });

  it('orders the pinned block by pin_order, unordered pins last in list order', () => {
    const pin = (id: string, pin_order: number | null) => ({ id, pin_order }) as RoomSummary;
    const ordered = orderPinned([pin('c', 2), pin('n1', null), pin('a', 0), pin('n2', null), pin('b', 1)]);
    expect(ordered.map((r) => r.id)).toEqual(['a', 'b', 'c', 'n1', 'n2']);
  });

  it('keeps the pinned chats in pin_order ahead of the unpinned ones', async () => {
    await roomsStore.start();
    await until(() => roomsStore.visible.length > 0);
    const pinnedIds = roomsStore.visible.filter((r) => r.pinned).map((r) => r.id);
    expect(pinnedIds.length).toBeGreaterThan(0);
    // Rewrite the order in reverse: the view must follow pin_order, not the engine's list order.
    const reversed = [...pinnedIds].reverse();
    for (const [index, id] of reversed.entries()) {
      const at = roomsStore.rooms.findIndex((r) => r.id === id);
      roomsStore.rooms[at] = { ...roomsStore.rooms[at], pin_order: index };
    }
    expect(roomsStore.visible.slice(0, reversed.length).map((r) => r.id)).toEqual(reversed);
    expect(roomsStore.visible.slice(reversed.length).some((r) => r.pinned)).toBe(false);
  });

  it('timeline opens a room, shows a local echo for a sent message and closes cleanly', async () => {
    await roomsStore.start();
    await until(() => roomsStore.rooms.length > 0);
    const room = roomsStore.rooms[0];
    await timelineStore.open(room.id);
    await until(() => timelineStore.items.length > 0);
    const before = timelineStore.items.length;

    await timelineStore.send('hello from the test');
    await until(() =>
      timelineStore.items.some(
        (i) => i.content.kind === 'TextMessage' && i.content.data.body === 'hello from the test'
      )
    );
    expect(timelineStore.items.length).toBeGreaterThan(before);

    await timelineStore.close();
    expect(timelineStore.items).toHaveLength(0);
    expect(timelineStore.roomId).toBeNull();
  });

  it('counts the mock Rust Forum and its topics as one unread chat', async () => {
    await roomsStore.start();
    await until(() => roomsStore.rooms.some((r) => r.is_forum));
    const forum = roomsStore.rooms.find((r) => r.is_forum);
    if (!forum) throw new Error('mock room list has no forum');
    const subtree = roomsStore.rooms.filter((r) => r.id === forum.id || r.parent_id === forum.id);
    expect(subtree.length).toBeGreaterThan(1);
    // Several rows carry unread topics; the header and the shell badge show the forum once.
    expect(unreadChats(subtree, (r) => roomsStore.isUnread(r))).toBe(1);
  });
});

/** Only the fields the counting rule reads; the rest is filler a room summary always carries. */
const chat = (id: string, patch: Partial<RoomSummary> = {}): RoomSummary =>
  ({
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
  }) as RoomSummary;

describe('unread chat count', () => {
  beforeEach(() => {
    roomsStore.rooms = [];
  });

  it('counts a forum with three unread topics and an unread forum as one chat', () => {
    const rooms = [
      chat('forum', { is_forum: true, unread_notifications: { highlight_count: 0, notification_count: 2 } }),
      chat('t1', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
      chat('t2', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
      chat('t3', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
      chat('dm', { kind: 'direct', is_direct: true }),
    ];
    expect(unreadChats(rooms, (r) => r.unread_notifications.notification_count > 0)).toBe(1);
    roomsStore.rooms = rooms;
    expect(roomsStore.unreadChatCount).toBe(1);
  });

  it('counts a read forum with one unread topic once', () => {
    const rooms = [
      chat('forum', { is_forum: true }),
      chat('t1', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 3 } }),
      chat('other', { unread_notifications: { highlight_count: 0, notification_count: 1 } }),
    ];
    expect(unreadChats(rooms, (r) => r.unread_notifications.notification_count > 0)).toBe(2);
    roomsStore.rooms = rooms;
    expect(roomsStore.unreadChatCount).toBe(2);
  });

  it('counts a topic on its own when its forum is archived or absent from the list', () => {
    const archivedForum = [
      chat('forum', { is_forum: true, archived: true }),
      chat('t1', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
    ];
    expect(unreadChats(archivedForum, (r) => r.unread_notifications.notification_count > 0)).toBe(1);

    // Filtered out, or not yet synced: the topic is a chat of its own and must not vanish.
    const absentForum = [
      chat('t1', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
    ];
    expect(unreadChats(absentForum, (r) => r.unread_notifications.notification_count > 0)).toBe(1);

    roomsStore.rooms = archivedForum;
    expect(roomsStore.unreadChatCount).toBe(1);
  });

  it('counts a marked-unread topic through its forum and skips archived chats', () => {
    roomsStore.rooms = [
      chat('forum', { is_forum: true }),
      chat('t1', { parent_id: 'forum', marked_unread: true }),
      chat('old', { archived: true, unread_notifications: { highlight_count: 0, notification_count: 4 } }),
    ];
    expect(roomsStore.unreadChatCount).toBe(1);
  });

  it('leaves the per-room badges alone: every row keeps its own count', () => {
    roomsStore.rooms = [
      chat('forum', { is_forum: true, unread_notifications: { highlight_count: 0, notification_count: 2 } }),
      chat('t1', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
      chat('t2', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
      chat('t3', { parent_id: 'forum', unread_notifications: { highlight_count: 0, notification_count: 1 } }),
    ];
    expect(roomsStore.rooms.map((r) => roomsStore.unreadOf(r))).toEqual([2, 1, 1, 1]);
    // The forum row still shows the sum of its topics (`forumTotals`); only the chat count folds.
    expect(roomsStore.rooms.reduce((n, r) => n + roomsStore.unreadOf(r), 0)).toBe(5);
    expect(roomsStore.unreadChatCount).toBe(1);
  });
});

describe('active room', () => {
  beforeEach(() => {
    roomsStore.rooms = [];
    roomsStore.select(null);
  });

  it('keeps the open topic known after the list drops it, so its forum row stays lit', () => {
    roomsStore.rooms = [chat('forum', { is_forum: true }), chat('t1', { parent_id: 'forum' })];
    roomsStore.select('t1');
    expect(roomsStore.active?.id).toBe('t1');
    // The sidebar highlights a forum through `parent_id`; a filter or a refresh that drops the
    // topic from the list must not take its parent with it.
    roomsStore.rooms = [chat('forum', { is_forum: true })];
    expect(roomsStore.active?.id).toBe('t1');
    expect(roomsStore.active?.parent_id).toBe('forum');
  });
});

describe('session account state', () => {
  it('needsRelogin follows the stored account state', async () => {
    await session.boot();
    const original = session.accounts;
    const [first] = original;
    session.accounts = [{ ...first, state: 'needs_relogin' }];
    expect(session.needsRelogin).toBe(true);
    session.accounts = [{ ...first, state: 'active' }];
    expect(session.needsRelogin).toBe(false);
    session.accounts = original;
  });

  it('logging out every account returns to onboarding', async () => {
    await session.boot();
    // The engine is authoritative; start from its list, not from a previous test's local upsert.
    await session.refreshAccounts();
    await session.logout();
    expect(session.accounts).toHaveLength(0);
    expect(session.phase).toBe('onboarding');
    expect(session.needsRelogin).toBe(false);
  });
});
