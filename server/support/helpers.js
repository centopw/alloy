/** Shared test helpers. Not a test file: node --test only collects *.test.js. */

/**
 * @param {(request: Request) => Promise<Response>} app
 * @param {string} path
 * @param {{ method?: string, token?: string, body?: BodyInit, headers?: Record<string, string> }} [init]
 * @returns {Promise<Response>}
 */
export function call(app, path, { method = 'GET', token, body, headers = {} } = {}) {
  const all = new Headers(headers);
  if (token !== undefined) all.set('authorization', `Bearer ${token}`);
  if (body !== undefined && !all.has('content-type')) all.set('content-type', 'application/octet-stream');
  const requestInit = /** @type {RequestInit} */ ({ method, headers: all });
  if (body !== undefined) requestInit.body = body;
  return app(new Request(`https://sync.test${path}`, requestInit));
}

/** @param {string} text @returns {Uint8Array} */
export function bytes(text) {
  return new TextEncoder().encode(text);
}

/** @returns {string} 43-char base64url id, the shape the server accepts. */
export function blobId() {
  return Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString('base64url');
}
