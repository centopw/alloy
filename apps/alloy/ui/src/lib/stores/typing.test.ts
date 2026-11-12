import { afterEach, describe, expect, it, vi } from 'vitest';
import type { EngineEvent } from '../generated/index.js';
import { client } from '../ipc/index.js';
import { TYPING_EXPIRY_MS, typingStore } from './typing.svelte.js';

const ROOM_A = 'chat-a';
const ROOM_B = 'chat-b';

afterEach(() => {
  for (const room of [ROOM_A, ROOM_B]) typingStore.apply(room, []);
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('typingStore.usersIn', () => {
  it('returns the users from the latest notice for the room', () => {
    typingStore.apply(ROOM_A, ['alice', 'bob']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice', 'bob']);
  });

  it('passes the notice through verbatim: the engine already leaves the account out', () => {
    typingStore.apply(ROOM_A, ['alice']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice']);
  });

  it('returns nothing for a room with no notice, a null room, or after the room is cleared', () => {
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
    expect(typingStore.usersIn(null)).toEqual([]);
    typingStore.apply(ROOM_A, ['alice']);
    typingStore.apply(ROOM_A, []);
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
  });

  it('does not expose the array it stores', () => {
    const users = ['alice'];
    typingStore.apply(ROOM_A, users);
    users.push('bob');
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice']);
  });
});

describe('typingStore joins and leaves', () => {
  it('replaces the set with each notice, so joins and leaves follow the engine', () => {
    typingStore.apply(ROOM_A, ['alice']);
    typingStore.apply(ROOM_A, ['alice', 'bob']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice', 'bob']);

    typingStore.apply(ROOM_A, ['bob']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['bob']);
  });

  it('removes the room entry once the last user stops typing', () => {
    typingStore.apply(ROOM_A, ['alice']);
    typingStore.apply(ROOM_A, []);
    expect(typingStore.rooms.has(ROOM_A)).toBe(false);
  });
});

describe('typingStore multiple rooms', () => {
  it('keeps each room separate', () => {
    typingStore.apply(ROOM_A, ['alice']);
    typingStore.apply(ROOM_B, ['bob']);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice']);
    expect(typingStore.usersIn(ROOM_B)).toEqual(['bob']);
  });

  it('clearing one room leaves the other untouched', () => {
    typingStore.apply(ROOM_A, ['alice']);
    typingStore.apply(ROOM_B, ['bob']);
    typingStore.apply(ROOM_A, []);
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
    expect(typingStore.usersIn(ROOM_B)).toEqual(['bob']);
  });
});

describe('typingStore expiry', () => {
  it('drops a room whose final notice never arrived', () => {
    vi.useFakeTimers();
    typingStore.apply(ROOM_A, ['alice']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice']);
    vi.advanceTimersByTime(1);
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
  });

  it('restarts the countdown on every notice', () => {
    vi.useFakeTimers();
    typingStore.apply(ROOM_A, ['alice']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1000);
    typingStore.apply(ROOM_A, ['alice']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice']);
  });

  it('cancels the countdown when an empty notice arrives', () => {
    vi.useFakeTimers();
    typingStore.apply(ROOM_A, ['alice']);
    typingStore.apply(ROOM_A, []);
    typingStore.apply(ROOM_A, ['bob']);
    vi.advanceTimersByTime(TYPING_EXPIRY_MS - 1);
    expect(typingStore.usersIn(ROOM_A)).toEqual(['bob']);
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

    handler?.({ type: 'TypingNotice', payload: { room_id: ROOM_A, users: ['alice'] } });
    expect(typingStore.usersIn(ROOM_A)).toEqual(['alice']);

    handler?.({ type: 'TypingNotice', payload: { room_id: ROOM_A, users: [] } });
    expect(typingStore.usersIn(ROOM_A)).toEqual([]);
  });
});
