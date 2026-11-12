import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PROFILE_TTL_MS, UserProfileStore } from './userProfile.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

const USER = 'user-42';
const ACCOUNT = 'acc-1';
const OTHER_ACCOUNT = 'acc-2';

function profile(dm: string | null = null) {
  return {
    user_id: USER,
    account_id: ACCOUNT,
    display_name: 'Bob',
    avatar: null,
    is_me: false,
    dm_room_id: dm,
  };
}

const profileRes = (dm: string | null = null) => ({
  type: 'UserProfile',
  payload: { profile: profile(dm) },
});

function deferred<T>() {
  const { promise, resolve } = Promise.withResolvers<T>();
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
  it('asks the account that knows the user', async () => {
    invoke.mockResolvedValueOnce(profileRes());
    const result = await store.get(ACCOUNT, USER);

    expect(invoke).toHaveBeenCalledWith({
      action: 'GetUserProfile',
      params: { account_id: ACCOUNT, user_id: USER },
    });
    expect(result.display_name).toBe('Bob');
  });

  it('shares one in-flight request for the same account and user', async () => {
    const answer = deferred<unknown>();
    invoke.mockReturnValueOnce(answer.promise);

    const a = store.get(ACCOUNT, USER);
    const b = store.get(ACCOUNT, USER);
    expect(invoke).toHaveBeenCalledTimes(1);

    answer.resolve(profileRes());
    await expect(Promise.all([a, b])).resolves.toEqual([profile(), profile()]);
  });

  it('serves a fresh answer from cache', async () => {
    invoke.mockResolvedValue(profileRes());
    await store.get(ACCOUNT, USER);
    await store.get(ACCOUNT, USER);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('refetches after the cache expires', async () => {
    vi.useFakeTimers();
    invoke.mockResolvedValue(profileRes());
    await store.get(ACCOUNT, USER);
    vi.advanceTimersByTime(PROFILE_TTL_MS);
    await store.get(ACCOUNT, USER);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('does not let one account answer for another', async () => {
    invoke.mockResolvedValue(profileRes());
    await store.get(ACCOUNT, USER);
    await store.get(OTHER_ACCOUNT, USER);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenLastCalledWith({
      action: 'GetUserProfile',
      params: { account_id: OTHER_ACCOUNT, user_id: USER },
    });
  });

  it('reports the direct chat the account already has, without inventing one', async () => {
    invoke.mockResolvedValueOnce(profileRes('dm-7'));
    const result = await store.get(ACCOUNT, USER);
    expect(result.dm_room_id).toBe('dm-7');
  });

  it('forgets one account on sign-out and keeps the other', async () => {
    invoke.mockResolvedValue(profileRes());
    await store.get(ACCOUNT, USER);
    await store.get(OTHER_ACCOUNT, USER);

    store.forgetAccount(ACCOUNT);
    await store.get(ACCOUNT, USER);
    await store.get(OTHER_ACCOUNT, USER);
    expect(invoke).toHaveBeenCalledTimes(3);
  });

  it('rejects when the engine cannot answer', async () => {
    invoke.mockRejectedValueOnce({ code: 'not_found', message: 'Unknown user' });
    await expect(store.get(ACCOUNT, USER)).rejects.toMatchObject({ code: 'not_found' });
  });
});
