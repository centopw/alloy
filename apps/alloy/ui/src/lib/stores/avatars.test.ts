import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MediaSource } from '../generated/index.js';
import {
  AVATAR_FAILURE_TTL_MS,
  avatarUrl,
  bucket,
  resolveAvatar,
  resetAvatarCache,
  setAvatarFetcher,
  type AvatarFetcher,
} from './avatars.svelte';

const SOURCE: MediaSource = { media_id: 'abc' };
const OTHER: MediaSource = { media_id: 'def' };

function stubFetcher(impl?: (source: MediaSource, px: number) => Promise<string>) {
  const calls: Array<{ source: MediaSource; px: number }> = [];
  const fn: AvatarFetcher = (source, px) => {
    calls.push({ source, px });
    return impl ? impl(source, px) : Promise.resolve(`asset://${source.media_id}/${px}`);
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
    await avatarUrl(SOURCE, 36);
    expect(calls).toEqual([{ source: SOURCE, px: 64 }]);
  });

  it('returns the fetched URL', async () => {
    stubFetcher();
    await expect(avatarUrl(SOURCE, 36)).resolves.toBe('asset://abc/64');
  });

  it('shares one in-flight fetch between concurrent requests in the same bucket', async () => {
    let release!: (url: string) => void;
    const calls = stubFetcher(() => new Promise<string>((r) => (release = r)));

    const a = avatarUrl(SOURCE, 20);
    const b = avatarUrl(SOURCE, 30);
    expect(calls).toHaveLength(1);

    release('asset://ok');
    await expect(Promise.all([a, b])).resolves.toEqual(['asset://ok', 'asset://ok']);
    expect(calls).toHaveLength(1);
  });

  it('fetches again for a different bucket or a different media object', async () => {
    const calls = stubFetcher();
    await avatarUrl(SOURCE, 36);
    await avatarUrl(SOURCE, 48); // same 64 bucket: served from cache
    await avatarUrl(SOURCE, 100);
    await avatarUrl(OTHER, 36);
    expect(calls.map((c) => c.px)).toEqual([64, 128, 64]);
  });

  it('serves successful results from cache without refetching', async () => {
    const calls = stubFetcher();
    await avatarUrl(SOURCE, 36);
    await avatarUrl(SOURCE, 40);
    vi.advanceTimersByTime(10 * AVATAR_FAILURE_TTL_MS);
    await avatarUrl(SOURCE, 36);
    expect(calls).toHaveLength(1);
  });

  it('resolves null without throwing when the fetch fails', async () => {
    stubFetcher(() => Promise.reject(new Error('boom')));
    await expect(avatarUrl(SOURCE, 36)).resolves.toBeNull();
  });

  it('caches a failure for 60 seconds, then retries', async () => {
    let fail = true;
    const calls = stubFetcher(() =>
      fail ? Promise.reject(new Error('boom')) : Promise.resolve('asset://ok')
    );

    await expect(avatarUrl(SOURCE, 36)).resolves.toBeNull();
    vi.advanceTimersByTime(AVATAR_FAILURE_TTL_MS - 1);
    await expect(avatarUrl(SOURCE, 36)).resolves.toBeNull();
    expect(calls).toHaveLength(1);

    fail = false;
    vi.advanceTimersByTime(1);
    await expect(avatarUrl(SOURCE, 36)).resolves.toBe('asset://ok');
    expect(calls).toHaveLength(2);
  });

  it('treats an empty URL from the fetcher as a failure', async () => {
    stubFetcher(() => Promise.resolve(''));
    await expect(avatarUrl(SOURCE, 36)).resolves.toBeNull();
  });
});

describe('resolveAvatar', () => {
  it('resolves null for a row without a picture, without asking the engine', async () => {
    const calls = stubFetcher();
    await expect(resolveAvatar(null, 36)).resolves.toBeNull();
    await expect(resolveAvatar(undefined, 36)).resolves.toBeNull();
    expect(calls).toHaveLength(0);
  });

  it('resolves a picture through the same cache', async () => {
    const calls = stubFetcher();
    await expect(resolveAvatar(SOURCE, 36)).resolves.toBe('asset://abc/64');
    await expect(resolveAvatar(SOURCE, 36)).resolves.toBe('asset://abc/64');
    expect(calls).toHaveLength(1);
  });
});
