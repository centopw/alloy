import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  AVATAR_FAILURE_TTL_MS,
  avatarUrl,
  bucket,
  resetAvatarCache,
  setAvatarFetcher,
  type AvatarFetcher,
} from './avatars.svelte';

const MXC = 'mxc://example.org/abc';

function stubFetcher(impl?: (mxc: string, px: number) => Promise<string>) {
  const calls: Array<{ mxc: string; px: number }> = [];
  const fn: AvatarFetcher = (mxc, px) => {
    calls.push({ mxc, px });
    return impl ? impl(mxc, px) : Promise.resolve(`asset://${mxc.replace('mxc://', '')}/${px}`);
  };
  setAvatarFetcher(fn);
  return calls;
}

beforeEach(() => {
  vi.useFakeTimers();
  resetAvatarCache();
});

afterEach(() => {
  setAvatarFetcher(null);
  resetAvatarCache();
  vi.useRealTimers();
});

describe('bucket', () => {
  it('rounds up to the smallest covering bucket', () => {
    expect(bucket(1)).toBe(32);
    expect(bucket(20)).toBe(32);
    expect(bucket(32)).toBe(32);
    expect(bucket(33)).toBe(64);
    expect(bucket(36)).toBe(64);
    expect(bucket(64)).toBe(64);
    expect(bucket(65)).toBe(128);
    expect(bucket(128)).toBe(128);
    expect(bucket(129)).toBe(256);
  });

  it('clamps to the largest bucket', () => {
    expect(bucket(256)).toBe(256);
    expect(bucket(4096)).toBe(256);
  });
});

describe('avatarUrl', () => {
  it('fetches at the bucket size, not the raw size', async () => {
    const calls = stubFetcher();
    await avatarUrl(MXC, 36);
    expect(calls).toEqual([{ mxc: MXC, px: 64 }]);
  });

  it('returns the fetched URL', async () => {
    stubFetcher();
    await expect(avatarUrl(MXC, 36)).resolves.toBe('asset://example.org/abc/64');
  });

  it('shares one in-flight fetch between concurrent requests in the same bucket', async () => {
    let release!: (url: string) => void;
    const calls = stubFetcher(() => new Promise<string>((r) => (release = r)));

    const a = avatarUrl(MXC, 20);
    const b = avatarUrl(MXC, 30);
    expect(calls).toHaveLength(1);

    release('asset://ok');
    await expect(Promise.all([a, b])).resolves.toEqual(['asset://ok', 'asset://ok']);
    expect(calls).toHaveLength(1);
  });

  it('fetches again for a different bucket', async () => {
    const calls = stubFetcher();
    await avatarUrl(MXC, 36);
    await avatarUrl(MXC, 48); // same 64 bucket: served from cache
    await avatarUrl(MXC, 100);
    expect(calls.map((c) => c.px)).toEqual([64, 128]);
  });

  it('serves successful results from cache without refetching', async () => {
    const calls = stubFetcher();
    await avatarUrl(MXC, 36);
    await avatarUrl(MXC, 40);
    vi.advanceTimersByTime(10 * AVATAR_FAILURE_TTL_MS);
    await avatarUrl(MXC, 36);
    expect(calls).toHaveLength(1);
  });

  it('resolves null without throwing when the fetch fails', async () => {
    stubFetcher(() => Promise.reject(new Error('boom')));
    await expect(avatarUrl(MXC, 36)).resolves.toBeNull();
  });

  it('caches a failure for 60 seconds, then retries', async () => {
    let fail = true;
    const calls = stubFetcher(() =>
      fail ? Promise.reject(new Error('boom')) : Promise.resolve('asset://ok')
    );

    await expect(avatarUrl(MXC, 36)).resolves.toBeNull();
    vi.advanceTimersByTime(AVATAR_FAILURE_TTL_MS - 1);
    await expect(avatarUrl(MXC, 36)).resolves.toBeNull();
    expect(calls).toHaveLength(1);

    fail = false;
    vi.advanceTimersByTime(1);
    await expect(avatarUrl(MXC, 36)).resolves.toBe('asset://ok');
    expect(calls).toHaveLength(2);
  });

  it('treats an empty URL from the fetcher as a failure', async () => {
    stubFetcher(() => Promise.resolve(''));
    await expect(avatarUrl(MXC, 36)).resolves.toBeNull();
  });

  it('returns null for non-mxc input without fetching', async () => {
    const calls = stubFetcher();
    await expect(avatarUrl('https://example.org/a.png', 36)).resolves.toBeNull();
    expect(calls).toHaveLength(0);
  });
});
