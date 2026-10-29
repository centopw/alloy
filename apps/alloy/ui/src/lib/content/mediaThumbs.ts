import { localFileUrl } from '$lib/platform/files';

/** How the engine was asked to serve one media source. */
export type MediaRequestKind = 'thumbnail' | 'file';

/**
 * Asks the engine for a media source and resolves the local path it was cached at, or `null` when
 * the engine refuses the request. Rejections are treated the same way by the cache below.
 */
export type MediaPathRequest = (
  uri: string,
  kind: MediaRequestKind,
  mimeType: string | null
) => Promise<string | null>;

/** Turns a served local path into a URL the webview can load. */
export type MediaUrlBuilder = (path: string) => Promise<string>;

const defaultBuild: MediaUrlBuilder = (path) => localFileUrl(path);

/**
 * Resolves the tile URLs of one room's shared media for the life of one panel.
 *
 * A thumbnail is an optimisation, not a requirement: the engine refuses some of them (a format it
 * cannot decode, an image without a pre-generated thumbnail), while the original file is still
 * served. A refused thumbnail therefore falls back to the full file once, and the tile shows it.
 *
 * Only a URL that was actually served is kept. A refusal — including a resolved `null` — is dropped
 * from the cache, so a later lookup retries instead of pinning the failure; a panel that reopens
 * starts from an empty cache, which is the retry point the room-info surface promises.
 */
export class MediaThumbCache {
  readonly #entries = new Map<string, Promise<string | null>>();
  readonly #request: MediaPathRequest;
  readonly #build: MediaUrlBuilder;

  constructor(request: MediaPathRequest, build: MediaUrlBuilder = defaultBuild) {
    this.#request = request;
    this.#build = build;
  }

  /** The tile URL for a source: its thumbnail, else the full file, else `null`. */
  thumbnail(uri: string, mimeType: string | null): Promise<string | null> {
    const key = `tile:${uri}`;
    const cached = this.#entries.get(key);
    if (cached) return cached;
    // Both questions are asked once, and the answer to the tile — thumbnail or fallback — is what
    // gets remembered, so a refused thumbnail is not asked for again on every tile.
    const promise = (async () => {
      const thumbnail = await this.#serve(`thumbnail:${uri}`, uri, 'thumbnail', mimeType);
      return thumbnail ?? this.#serve(`file:${uri}`, uri, 'file', mimeType);
    })().then((url) => {
      if (!url && this.#entries.get(key) === promise) this.#entries.delete(key);
      return url;
    });
    this.#entries.set(key, promise);
    return promise;
  }

  /** The original file's URL; also the retry path when the webview rejects a thumbnail. */
  fullFile(uri: string, mimeType: string | null): Promise<string | null> {
    return this.#serve(`file:${uri}`, uri, 'file', mimeType);
  }

  /** Drops every entry, so a reopened panel tries again. */
  clear(): void {
    this.#entries.clear();
  }

  #serve(key: string, uri: string, kind: MediaRequestKind, mimeType: string | null) {
    const cached = this.#entries.get(key);
    if (cached) return cached;
    // Concurrent tiles asking for the same source share one request.
    const promise = this.#resolve(uri, kind, mimeType).then((url) => {
      // A refusal is not a result: forget it so the next lookup can succeed.
      if (!url && this.#entries.get(key) === promise) this.#entries.delete(key);
      return url;
    });
    this.#entries.set(key, promise);
    return promise;
  }

  async #resolve(uri: string, kind: MediaRequestKind, mimeType: string | null): Promise<string | null> {
    try {
      const path = await this.#request(uri, kind, mimeType);
      return path ? await this.#build(path) : null;
    } catch {
      // The tile has a fallback of its own; a refusal is not an error to surface.
      return null;
    }
  }
}
