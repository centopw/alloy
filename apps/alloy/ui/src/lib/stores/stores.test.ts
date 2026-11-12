import { describe, it, expect, beforeEach } from 'vitest';
import { Subscription } from './subscription';
import { roomsStore, roomTitle } from './rooms.svelte';
import { timelineStore } from './timeline.svelte';
import { session } from './session.svelte';
import type { EngineEvent } from '../generated/index.js';

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
