/**
 * @module core - the single request handler shared by every deployment.
 *
 * It is deliberately dumb: the server stores opaque bytes under opaque names and
 * enforces compare-and-swap on `head`, `escrow` and `roster`. It never parses,
 * decrypts, or interprets a body, and never learns a plaintext identifier.
 *
 * @typedef {object} Store
 * @property {(key: string) => Promise<Uint8Array | null>} get
 * @property {(key: string, value: Uint8Array) => Promise<void>} put
 * @property {(key: string) => Promise<void>} delete
 * @property {(prefix: string) => Promise<string[]>} listPrefix
 */

import { concat, constantTimeEqual, hexOf, sha256, utf8 } from './bytes.js';

const OCTET_TYPE = 'application/octet-stream';
const DEFAULT_MAX_BODY_BYTES = 1024 * 1024;
/** Opaque blob id: 32 random bytes, base64url. Shape only, never decoded. */
const BLOB_ID_SHAPE = /^[A-Za-z0-9_-]{43}$/;
/** Compare-and-swap resources, keyed by pathname. */
const CAS_PATHS = new Map([
  ['/v1/head', 'head'],
  ['/v1/escrow', 'escrow'],
  ['/v1/roster', 'roster'],
]);

class HttpError extends Error {
  /**
   * @param {number} status
   * @param {Record<string, string>} [headers]
   */
  constructor(status, headers = {}) {
    super(`http ${status}`);
    this.status = status;
    this.headers = headers;
  }
}

/**
 * @param {object} config
 * @param {Store} config.store
 * @param {{ token: string, quota: number }[]} config.tokens
 * @param {string} config.version
 * @param {((line: string) => void) | null} [config.log] called with method, status, duration only
 * @param {number} [config.maxBodyBytes]
 * @returns {Promise<(request: Request) => Promise<Response>>}
 */
export async function createApp({
  store,
  tokens,
  version,
  log = null,
  maxBodyBytes = DEFAULT_MAX_BODY_BYTES,
}) {
  if (!store) throw new Error('store is required');
  if (!Array.isArray(tokens) || tokens.length === 0) throw new Error('at least one token is required');

  const known = await Promise.all(
    tokens.map(async (entry) => ({ digest: await sha256(utf8(entry.token)), quota: entry.quota })),
  );
  /** Serialises read-check-write so a CAS has exactly one winner. */
  const serialize = createMutex();

  /** @param {Request} request @returns {Promise<{ digest: Uint8Array, quota: number } | null>} */
  async function authenticate(request) {
    const header = request.headers.get('authorization');
    if (!header || !header.startsWith('Bearer ')) return null;
    const digest = await sha256(utf8(header.slice('Bearer '.length)));
    /** @type {{ digest: Uint8Array, quota: number } | null} */
    let found = null;
    for (const candidate of known) {
      // No early exit: every configured token is compared on every request.
      if (constantTimeEqual(candidate.digest, digest)) found = candidate;
    }
    return found;
  }

  /**
   * Charge bytes to a token's quota before the payload is written. The raw token is
   * never a storage key.
   * @param {{ digest: Uint8Array, quota: number }} owner
   * @param {number} bytes
   * @returns {Promise<boolean>}
   */
  async function reserve(owner, bytes) {
    const key = `usage/${hexOf(owner.digest)}`;
    const raw = await store.get(key);
    const used = raw ? Number(new TextDecoder().decode(raw)) : 0;
    const total = (Number.isFinite(used) ? used : 0) + bytes;
    if (total > owner.quota) return false;
    await store.put(key, utf8(String(total)));
    return true;
  }

  /** @param {Uint8Array} bytes @returns {Promise<string>} */
  async function etagOf(bytes) {
    return `"${hexOf(await sha256(bytes))}"`;
  }

  /**
   * @param {Request} request
   * @param {number} cap
   * @returns {Promise<Uint8Array>}
   */
  async function readCapped(request, cap) {
    const declared = request.headers.get('content-length');
    if (declared !== null) {
      const size = Number(declared);
      if (!Number.isSafeInteger(size) || size < 0) throw new HttpError(400);
      // A truthful-looking-but-large length is rejected before a byte is read.
      if (size > cap) {
        if (request.body) await request.body.cancel().catch(() => {});
        throw new HttpError(413);
      }
    }
    if (!request.body) return new Uint8Array(0);

    const reader = request.body.getReader();
    /** @type {Uint8Array[]} */
    const chunks = [];
    let total = 0;
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        total += value.byteLength;
        if (total > cap) {
          // Stop pulling: a lying Content-Length cannot make the server buffer past the cap.
          await reader.cancel().catch(() => {});
          throw new HttpError(413);
        }
        chunks.push(value);
      }
    } finally {
      reader.releaseLock();
    }
    return concat(chunks, total);
  }

  /** @param {Request} request */
  function requireOctetStream(request) {
    const type = (request.headers.get('content-type') ?? '').split(';')[0].trim().toLowerCase();
    if (type !== OCTET_TYPE) throw new HttpError(415);
  }

  /** @param {string} key @returns {Promise<Response>} */
  async function readResource(key) {
    const value = await store.get(key);
    if (!value) return new Response(null, { status: 404 });
    return new Response(value, {
      status: 200,
      headers: { 'content-type': OCTET_TYPE, etag: await etagOf(value) },
    });
  }

  /**
   * @param {Request} request
   * @param {string} id
   * @param {{ digest: Uint8Array, quota: number }} token
   * @returns {Promise<Response>}
   */
  async function writeBlob(request, id, token) {
    requireOctetStream(request);
    const body = await readCapped(request, maxBodyBytes);
    return serialize(async () => {
      if (!(await reserve(token, body.byteLength))) return new Response(null, { status: 429 });
      await store.put(`blob/${id}`, body);
      return new Response(null, { status: 204, headers: { etag: await etagOf(body) } });
    });
  }

  /**
   * `DELETE /v1/blob/{id}`. The bytes are already accounted to the token's quota; deleting never
   * refunds it, so a client cannot free quota by rewriting and deleting in a loop.
   * @param {string} id
   * @returns {Promise<Response>}
   */
  async function deleteBlob(id) {
    return serialize(async () => {
      const key = `blob/${id}`;
      // The server cannot tell an already-deleted id from one that never existed: both are 404.
      if ((await store.get(key)) === null) return new Response(null, { status: 404 });
      await store.delete(key);
      return new Response(null, { status: 204 });
    });
  }

  /**
   * @param {Request} request
   * @param {string} key
   * @param {{ digest: Uint8Array, quota: number }} token
   * @returns {Promise<Response>}
   */
  async function writeResource(request, key, token) {
    const ifMatch = parseIfMatch(request.headers);
    const ifNoneMatchAny = parseIfNoneMatchAny(request.headers);
    if (ifMatch === null && !ifNoneMatchAny) throw new HttpError(428);
    requireOctetStream(request);
    const body = await readCapped(request, maxBodyBytes);

    return serialize(async () => {
      const current = await store.get(key);
      const currentEtag = current ? await etagOf(current) : null;

      if (ifMatch !== null) {
        if (currentEtag === null || !ifMatch.includes(currentEtag)) {
          // Stale or absent head: the caller must rebase from the body/ETag returned here.
          /** @type {Record<string, string>} */
          const headers = { 'content-type': OCTET_TYPE };
          if (currentEtag) headers.etag = currentEtag;
          return new Response(current ?? null, { status: 409, headers });
        }
      } else if (current !== null) {
        // If-None-Match: * is create-only.
        return new Response(null, { status: 412 });
      }

      if (!(await reserve(token, body.byteLength))) return new Response(null, { status: 429 });
      await store.put(key, body);
      return new Response(null, { status: 204, headers: { etag: await etagOf(body) } });
    });
  }

  /** @param {Request} request @returns {Promise<Response>} */
  async function route(request) {
    const { pathname } = new URL(request.url);
    const method = request.method;

    if (pathname === '/healthz') {
      if (method !== 'GET') throw new HttpError(405, { allow: 'GET' });
      return new Response(JSON.stringify({ ok: true, version }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }

    const token = await authenticate(request);
    if (!token) return new Response(null, { status: 401, headers: { 'www-authenticate': 'Bearer' } });

    const casKey = CAS_PATHS.get(pathname);
    if (casKey) {
      if (method === 'GET') return readResource(casKey);
      if (method === 'PUT') return writeResource(request, casKey, token);
      throw new HttpError(405, { allow: 'GET, PUT' });
    }

    if (pathname.startsWith('/v1/blob/')) {
      const id = pathname.slice('/v1/blob/'.length);
      // Shape only; a traversal-looking or percent-encoded id never reaches a store.
      if (!BLOB_ID_SHAPE.test(id)) throw new HttpError(400);
      if (method === 'GET') return readResource(`blob/${id}`);
      if (method === 'PUT') return writeBlob(request, id, token);
      if (method === 'DELETE') return deleteBlob(id);
      throw new HttpError(405, { allow: 'GET, PUT, DELETE' });
    }

    throw new HttpError(404);
  }

  return async function handler(request) {
    const started = performance.now();
    /** @type {Response} */
    let response;
    try {
      response = await route(request);
    } catch (error) {
      response =
        error instanceof HttpError
          ? new Response(null, { status: error.status, headers: error.headers })
          : new Response(null, { status: 500 });
    }
    // Method, status and duration only: never a path, id, body, token or header value.
    if (log) log(`${request.method} ${response.status} ${Math.round(performance.now() - started)}ms`);
    return response;
  };
}

/** @param {Headers} headers @returns {string[] | null} */
function parseIfMatch(headers) {
  const value = headers.get('if-match');
  if (value === null) return null;
  return value
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

/** @param {Headers} headers @returns {boolean} */
function parseIfNoneMatchAny(headers) {
  const value = headers.get('if-none-match');
  if (value === null) return false;
  return value
    .split(',')
    .some((entry) => entry.trim() === '*');
}

/** @returns {<T>(task: () => Promise<T>) => Promise<T>} */
function createMutex() {
  /** @type {Promise<unknown>} */
  let tail = Promise.resolve();
  return (task) => {
    const run = tail.then(task, task);
    tail = run.then(
      () => {},
      () => {},
    );
    return run;
  };
}
