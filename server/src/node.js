/**
 * @module node - Node http adapter. Converts Node requests to Web Requests, calls the
 * shared handler from core.js, and streams the Web Response back. No routing here.
 */

import { createServer as createHttpServer } from 'node:http';
import { Readable } from 'node:stream';

/**
 * @param {import('node:http').IncomingMessage} req
 * @returns {Request}
 */
function toWebRequest(req) {
  const method = req.method ?? 'GET';
  /** @type {HeadersInit} */
  const headers = {};
  for (const [name, value] of Object.entries(req.headers)) {
    if (value === undefined) continue;
    headers[name] = Array.isArray(value) ? value.join(', ') : value;
  }

  /** @type {RequestInit} */
  const init = { method, headers };
  if (method !== 'GET' && method !== 'HEAD') {
    init.body = /** @type {any} */ (Readable.toWeb(req));
    init.duplex = 'half';
  }
  return new Request(`http://${req.headers.host ?? 'localhost'}${req.url ?? '/'}`, init);
}

/**
 * @param {(request: Request) => Promise<Response>} app
 * @returns {import('node:http').Server}
 */
export function createServer(app) {
  return createHttpServer(async (req, res) => {
    try {
      const request = toWebRequest(req);
      const response = await app(request);
      res.statusCode = response.status;
      response.headers.forEach((value, name) => res.setHeader(name, value));
      if (!response.body) {
        res.end();
        return;
      }
      Readable.fromWeb(/** @type {any} */ (response.body)).pipe(res);
    } catch {
      if (!res.headersSent) {
        res.statusCode = 500;
        res.end();
      } else {
        res.destroy();
      }
    }
  });
}
