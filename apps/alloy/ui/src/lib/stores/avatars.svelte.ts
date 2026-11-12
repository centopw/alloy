import { client } from '../ipc/index.js';
import type { MediaSource } from '../generated/index.js';
import { localFileUrl } from '../platform/files';

/** Thumbnail sizes (CSS px at 1x) that avatar images are fetched at. */
const BUCKETS = [32, 64, 128, 256] as const;

/** A failed lookup is remembered this long before it is retried. */
export const AVATAR_FAILURE_TTL_MS = 60_000;

/** Smallest bucket that covers `px`; anything above the largest bucket maps to it. */
export function bucket(px: number): number {
  for (const b of BUCKETS) {
    if (px <= b) return b;
  }
  return BUCKETS[BUCKETS.length - 1];
}

/** Resolves a stored media object to a webview-loadable URL at `px` (already bucketed) CSS pixels. Throws on failure. */
export type AvatarFetcher = (source: MediaSource, px: number) => Promise<string>;

const defaultFetcher: AvatarFetcher = async (source, px) => {
  const res = await client.invoke({
    action: 'GetMedia',
    params: { source, kind: 'thumbnail', mime_type: null },
  });
  if (res.type !== 'Media') throw new Error(`unexpected response: ${res.type}`);
  return await localFileUrl(res.payload.path);
};

let fetcher: AvatarFetcher = defaultFetcher;

/** Replace the thumbnail fetcher (tests). Pass null to restore the IPC-backed default. */
export function setAvatarFetcher(next: AvatarFetcher | null): void {
  fetcher = next ?? defaultFetcher;
}

interface Settled {
  url: string | null;
  expiresAt: number;
}

const settled = new Map<string, Settled>();
const inflight = new Map<string, Promise<string | null>>();

/** Drop every cached and in-flight avatar lookup (tests). */
export function resetAvatarCache(): void {
  settled.clear();
  inflight.clear();
}

async function load(source: MediaSource, px: number): Promise<string | null> {
  try {
    return (await fetcher(source, px)) || null;
  } catch {
    return null;
  }
}

/**
 * Resolve a stored media object to a URL the webview can load, sized for `px` CSS pixels.
 * Successes are cached for the session; failures are cached for AVATAR_FAILURE_TTL_MS.
 * Concurrent requests for the same key share one fetch. Never rejects: resolves null on failure.
 */
export function avatarUrl(source: MediaSource, px: number): Promise<string | null> {
  const size = bucket(px);
  const key = `${source.media_id}|${size}`;

  const hit = settled.get(key);
  if (hit) {
    if (Date.now() < hit.expiresAt) return Promise.resolve(hit.url);
    settled.delete(key);
  }

  const pending = inflight.get(key);
  if (pending) return pending;

  const request: Promise<string | null> = load(source, size)
    .then((url) => {
      settled.set(key, {
        url,
        expiresAt: url === null ? Date.now() + AVATAR_FAILURE_TTL_MS : Number.POSITIVE_INFINITY,
      });
      return url;
    })
    .finally(() => {
      if (inflight.get(key) === request) inflight.delete(key);
    });

  inflight.set(key, request);
  return request;
}

/** Resolve one chat or user avatar, or null when the row has none. */
export function resolveAvatar(source: MediaSource | null | undefined, px: number): Promise<string | null> {
  return source ? avatarUrl(source, px) : Promise.resolve(null);
}
