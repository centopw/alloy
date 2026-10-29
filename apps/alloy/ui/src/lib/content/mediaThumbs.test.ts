import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { MediaThumbCache, type MediaRequestKind } from './mediaThumbs';

/**
 * #115: the shared-media grid showed broken placeholders although the picture opened fine. Two
 * causes are covered here — a refused thumbnail must fall back to the original file instead of
 * pinning a `null`, and the tile must actually lay the picture over its placeholder.
 */

const build = (path: string) => Promise.resolve(`asset://${path}`);

/** A request double answering per kind; throw to simulate a refusal that rejects. */
function requestDouble(answer: Partial<Record<MediaRequestKind, string | null | Error>>) {
  return vi.fn(async (_uri: string, kind: MediaRequestKind) => {
    const value = answer[kind] ?? null;
    if (value instanceof Error) throw value;
    return value;
  });
}

describe('media thumbnails', () => {
  it('shows the thumbnail when the engine serves one', async () => {
    const request = requestDouble({ thumbnail: '/cache/thumb.jpg' });
    const cache = new MediaThumbCache(request, build);

    await expect(cache.thumbnail('mxc://a/1', 'image/jpeg')).resolves.toBe('asset:///cache/thumb.jpg');
    await expect(cache.thumbnail('mxc://a/1', 'image/jpeg')).resolves.toBe('asset:///cache/thumb.jpg');
    expect(request).toHaveBeenCalledTimes(1);
  });

  it('falls back to the original file when the thumbnail is refused', async () => {
    const request = requestDouble({ thumbnail: null, file: '/cache/full.png' });
    const cache = new MediaThumbCache(request, build);

    await expect(cache.thumbnail('mxc://a/2', 'image/heic')).resolves.toBe('asset:///cache/full.png');
    expect(request.mock.calls.map(([, kind]) => kind)).toEqual(['thumbnail', 'file']);

    // The fallback is the cached answer; a second tile does not ask again.
    await expect(cache.thumbnail('mxc://a/2', 'image/heic')).resolves.toBe('asset:///cache/full.png');
    expect(request).toHaveBeenCalledTimes(2);
  });

  it('does not pin a resolved null: the next lookup retries', async () => {
    const request = requestDouble({ thumbnail: null, file: null });
    const cache = new MediaThumbCache(request, build);

    await expect(cache.thumbnail('mxc://a/3', 'image/jpeg')).resolves.toBeNull();
    expect(request).toHaveBeenCalledTimes(2);

    await expect(cache.thumbnail('mxc://a/3', 'image/jpeg')).resolves.toBeNull();
    expect(request).toHaveBeenCalledTimes(4);
  });

  it('treats a rejecting request as a refusal and still reaches the file', async () => {
    const request = requestDouble({ thumbnail: new Error('M_UNKNOWN'), file: '/cache/full.jpg' });
    const cache = new MediaThumbCache(request, build);

    await expect(cache.thumbnail('mxc://a/4', 'image/jpeg')).resolves.toBe('asset:///cache/full.jpg');
  });

  it('forgets everything when the panel reopens', async () => {
    const answer: { thumbnail: string | null; file: string | null } = { thumbnail: null, file: null };
    const request = vi.fn(async (_uri: string, kind: MediaRequestKind) => answer[kind]);
    const cache = new MediaThumbCache(request, build);

    await expect(cache.thumbnail('mxc://a/5', 'image/jpeg')).resolves.toBeNull();
    answer.file = '/cache/full.jpg';
    cache.clear();

    await expect(cache.thumbnail('mxc://a/5', 'image/jpeg')).resolves.toBe('asset:///cache/full.jpg');
  });

  it('shares one request between tiles that ask for the same source', async () => {
    const request = requestDouble({ thumbnail: '/cache/thumb.jpg' });
    const cache = new MediaThumbCache(request, build);

    const [first, second] = await Promise.all([
      cache.thumbnail('mxc://a/6', 'image/jpeg'),
      cache.thumbnail('mxc://a/6', 'image/jpeg'),
    ]);
    expect(first).toBe(second);
    expect(request).toHaveBeenCalledTimes(1);
  });

  it('serves the original file for the rejected-thumbnail retry', async () => {
    const request = requestDouble({ thumbnail: '/cache/thumb.jpg', file: '/cache/full.jpg' });
    const cache = new MediaThumbCache(request, build);

    await expect(cache.thumbnail('mxc://a/7', 'image/jpeg')).resolves.toBe('asset:///cache/thumb.jpg');
    await expect(cache.fullFile('mxc://a/7', 'image/jpeg')).resolves.toBe('asset:///cache/full.jpg');
    await expect(cache.fullFile('mxc://a/7', 'image/jpeg')).resolves.toBe('asset:///cache/full.jpg');
    expect(request).toHaveBeenCalledTimes(2);
  });
});

describe('shared-media tiles', () => {
  const panelSource = readFileSync(
    resolve(process.cwd(), 'src/lib/components/shell/RoomInfoPanel.svelte'),
    'utf8'
  );

  it('retries an image the webview rejected once, then settles on the kind icon', () => {
    expect(panelSource).toContain('onerror={() => void imageRejected(item, url)}');
    expect(panelSource).toContain('if (retried.has(item.event_id))');
    expect(panelSource).toContain('await thumbs.fullFile(');
    expect(panelSource).toContain("imageFailed[item.event_id] = true");
    expect(panelSource).toContain('<Icon icon={File} size={20} />');
  });

  it('drops the old module-wide cache of thumbnails', () => {
    // A module-level map survived every panel and kept a refused thumbnail pinned forever.
    expect(panelSource).not.toContain('const thumbCache = new Map');
    expect(panelSource).toContain('const thumbs = createThumbCache();');
  });

  it('lays the picture over the placeholder instead of below it', () => {
    // Both layers fill the clipped cell; in normal flow the 100%-height placeholder pushed the
    // picture out of view, so a loaded image still showed the placeholder.
    const imageRule = panelSource.match(/\n {2}\.media-img \{[^}]*\}/)?.[0] ?? '';
    const fallbackRule = panelSource.match(/\n {2}\.media-fallback \{[^}]*\}/)?.[0] ?? '';
    expect(imageRule).toContain('position: absolute');
    expect(imageRule).toContain('inset: 0');
    expect(fallbackRule).toContain('position: absolute');
  });
});
