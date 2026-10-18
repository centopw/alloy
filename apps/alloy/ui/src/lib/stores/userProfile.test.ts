import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PROFILE_TTL_MS, UserProfileStore } from './userProfile.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

const USER = '@bob:alloy.chat';
const ROOM = '!general:alloy.chat';

function profile(dm: string | null = null) {
  return {
    user_id: USER,
    display_name: 'Bob',
    avatar_url: null,
    membership: 'join',
    power_level: 50,
    dm_room_id: dm,
    is_me: false,
  };
}

const profileRes = (dm: string | null = null) => ({
  type: 'UserProfile',
  payload: { profile: profile(dm) },
});

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

let store: UserProfileStore;

beforeEach(() => {
  invoke.mockReset();
  store = new UserProfileStore();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('UserProfileStore.get', () => {
  it('fetches the profile for a room and returns it', async () => {
    invoke.mockResolvedValueOnce(profileRes());
    const result = await store.get(USER, ROOM);

    expect(invoke).toHaveBeenCalledWith({
      action: 'GetUserProfile',
      params: { user_id: USER, room_id: ROOM },
    });
    expect(result.display_name).toBe('Bob');
  });

  it('passes a null room id through for profiles outside a room', async () => {
    invoke.mockResolvedValueOnce(profileRes());
    await store.get(USER, null);

    expect(invoke).toHaveBeenCalledWith({
      action: 'GetUserProfile',
      params: { user_id: USER, room_id: null },
    });
  });

  it('serves a repeat lookup from cache within the TTL', async () => {
    invoke.mockResolvedValueOnce(profileRes());
    await store.get(USER, ROOM);
    await store.get(USER, ROOM);

    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('refetches once the TTL has passed', async () => {
    vi.useFakeTimers();
    invoke.mockResolvedValueOnce(profileRes()).mockResolvedValueOnce(profileRes());
    await store.get(USER, ROOM);

    vi.advanceTimersByTime(PROFILE_TTL_MS + 1);
    await store.get(USER, ROOM);

    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('caches per room, so the same user in another room is fetched again', async () => {
    invoke.mockResolvedValueOnce(profileRes()).mockResolvedValueOnce(profileRes());
    await store.get(USER, ROOM);
    await store.get(USER, null);

    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('shares one request between concurrent lookups', async () => {
    const d = deferred<unknown>();
    invoke.mockReturnValueOnce(d.promise);

    const a = store.get(USER, ROOM);
    const b = store.get(USER, ROOM);
    expect(invoke).toHaveBeenCalledTimes(1);

    d.resolve(profileRes());
    await expect(Promise.all([a, b])).resolves.toHaveLength(2);
  });

  it('rejects when the reply is not a UserProfile and does not cache it', async () => {
    invoke.mockResolvedValueOnce({ type: 'Unit' });
    await expect(store.get(USER, ROOM)).rejects.toThrow('Expected UserProfile, got Unit');

    invoke.mockResolvedValueOnce(profileRes());
    await store.get(USER, ROOM);
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});

describe('UserProfileStore.startDirectMessage', () => {
  it('returns the room id of the opened direct message', async () => {
    invoke.mockResolvedValueOnce({
      type: 'DirectMessageRoom',
      payload: { room_id: '!dm:alloy.chat', created: true },
    });

    await expect(store.startDirectMessage(USER)).resolves.toBe('!dm:alloy.chat');
    expect(invoke).toHaveBeenCalledWith({
      action: 'StartDirectMessage',
      params: { user_id: USER },
    });
  });

  it('shares one request between concurrent calls for the same user', async () => {
    const d = deferred<unknown>();
    invoke.mockReturnValueOnce(d.promise);

    const a = store.startDirectMessage(USER);
    const b = store.startDirectMessage(USER);
    expect(invoke).toHaveBeenCalledTimes(1);

    d.resolve({ type: 'DirectMessageRoom', payload: { room_id: '!dm:alloy.chat', created: false } });
    await expect(Promise.all([a, b])).resolves.toEqual(['!dm:alloy.chat', '!dm:alloy.chat']);
  });

  it('drops the cached profile so its dm_room_id is re-read', async () => {
    invoke.mockResolvedValueOnce(profileRes(null));
    await store.get(USER, ROOM);

    invoke.mockResolvedValueOnce({
      type: 'DirectMessageRoom',
      payload: { room_id: '!dm:alloy.chat', created: true },
    });
    await store.startDirectMessage(USER);

    invoke.mockResolvedValueOnce(profileRes('!dm:alloy.chat'));
    const refreshed = await store.get(USER, ROOM);

    expect(invoke).toHaveBeenCalledTimes(3);
    expect(refreshed.dm_room_id).toBe('!dm:alloy.chat');
  });

  it('rejects on failure and leaves the cached profile in place', async () => {
    invoke.mockResolvedValueOnce(profileRes(null));
    await store.get(USER, ROOM);

    invoke.mockRejectedValueOnce(new Error('denied'));
    await expect(store.startDirectMessage(USER)).rejects.toThrow('denied');

    await store.get(USER, ROOM);
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});
