import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Account, EngineEvent } from '../generated/index.js';
import { client } from '../ipc/index.js';
import { session } from './session.svelte.js';
import { TYPING_EXPIRY_MS, typingStore } from './typing.svelte.js';

const ME = '@me:alloy.test';
const ROOM_A = '!a:alloy.test';
const ROOM_B = '!b:alloy.test';

const account: Account = {
  id: 'acc-1',
  user_id: ME,
  display_name: null,
  avatar_url: null,
  homeserver_url: 'https://alloy.test',
};

afterEach(() => {
  for (const room of [ROOM_A, ROOM_B]) typingStore.apply(room, []);
  session.account = null;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('typingStore.usersIn', () => {
  it('returns the users from the latest notice for the room', () => {
    typingStore.apply(ROOM_A, ['@alice:alloy.test', '@bob:alloy.test']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test', '@bob:alloy.test']);
  });

  it('excludes the signed-in user', () => {
    session.account = account;
    typingStore.apply(ROOM_A, [ME, '@alice:alloy.test']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test']);
  });

  it('excludes nobody when no account is signed in', () => {
    session.account = null;
    typingStore.apply(ROOM_A, [ME]);
    expect(typingStore.usersIn(ROOM_A)).toEqual([ME]);
  });

  it('returns nothing when only the signed-in user is typing', () => {
    session.account = account;
    typingStore.apply(ROOM_A, [ME]);
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
  });

  it('returns nothing for a room with no notice, a null room, or after the room is cleared', () => {
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
    expect(typingStore.usersIn(null)).toEqual([]);
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    typingStore.apply(ROOM_A, []);
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
  });

  it('does not expose the array it stores', () => {
    const users = ['@alice:alloy.test'];
    typingStore.apply(ROOM_A, users);
    users.push('@bob:alloy.test');
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test']);
  });
});

describe('typingStore joins and leaves', () => {
  it('replaces the set with each notice, so joins and leaves follow the engine', () => {
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    typingStore.apply(ROOM_A, ['@alice:alloy.test', '@bob:alloy.test']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test', '@bob:alloy.test']);

    typingStore.apply(ROOM_A, ['@bob:alloy.test']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@bob:alloy.test']);
  });

  it('removes the room entry once the last user stops typing', () => {
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    typingStore.apply(ROOM_A, []);
    expect(typingStore.rooms.has(ROOM_A)).toBe(false);
  });
});

describe('typingStore multiple rooms', () => {
  it('keeps each room separate', () => {
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    typingStore.apply(ROOM_B, ['@bob:alloy.test']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test']);
    expect(typingStore.usersIn(ROOM_B)).toEqual(['@bob:alloy.test']);
  });

  it('clearing one room leaves the other untouched', () => {
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    typingStore.apply(ROOM_B, ['@bob:alloy.test']);
    typingStore.apply(ROOM_A, []);
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
    expect(typingStore.usersIn(ROOM_B)).toEqual(['@bob:alloy.test']);
  });
});

describe('typingStore expiry', () => {
  it('drops a room whose final notice never arrived', () => {
    vi.useFakeTimers();
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test']);
    vi.advanceTimersByTime(1);
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
  });

  it('restarts the countdown on every notice', () => {
    vi.useFakeTimers();
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1000);
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test']);
  });

  it('cancels the countdown when an empty notice arrives', () => {
    vi.useFakeTimers();
    typingStore.apply(ROOM_A, ['@alice:alloy.test']);
    typingStore.apply(ROOM_A, []);
    typingStore.apply(ROOM_A, ['@bob:alloy.test']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@bob:alloy.test']);
  });
});

describe('typingStore.start', () => {
  it('subscribes once to the engine event stream and applies TypingNotice events', () => {
    let handler: ((event: EngineEvent) => void) | undefined;
    const subscribe = vi.spyOn(client, 'subscribe').mockImplementation((h) => {
      handler = h as (event: EngineEvent) => void;
      return () => {};
    });

    typingStore.start();
    typingStore.start();
    expect(subscribe).toHaveBeenCalledTimes(1);

    handler?.({ type: 'TypingNotice', payload: { room_id: ROOM_A, users: ['@alice:alloy.test'] } });
    expect(typingStore.usersIn(ROOM_A)).toEqual(['@alice:alloy.test']);

    handler?.({ type: 'TypingNotice', payload: { room_id: ROOM_A, users: [] } });
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
  });
});
