/**
 * @module worker - Cloudflare Workers entry. Routing lives entirely in core.js; this
 * file only turns bindings into a store, a token list and a version.
 *
 * wrangler.toml configures: SYNC_KV (KV binding), SYNC_TOKENS (secret, comma separated
 * `token:quota_bytes`), optional SYNC_LOG=1.
 */

import { createApp } from './core.js';
import { kvStore } from './kv-store.js';
import { parseTokenSpec } from './tokens.js';
import { VERSION } from './version.js';

/** @type {Promise<(request: Request) => Promise<Response>> | null} */
let appPromise = null;

/**
 * Built once per isolate. Workers KV is per-isolate consistent within that window,
 * which is also what the CAS mutex relies on.
 * @param {Record<string, any>} env
 */
function app(env) {
  if (appPromise === null) {
    appPromise = (async () => {
      if (!env.SYNC_KV) throw new Error('SYNC_KV binding is required');
      const tokens = parseTokenSpec(env.SYNC_TOKENS);
      if (tokens.length === 0) throw new Error('SYNC_TOKENS must configure at least one token');
      return createApp({
        store: kvStore(env.SYNC_KV),
        tokens,
        version: VERSION,
        log: env.SYNC_LOG === '1' ? (line) => console.log(line) : null,
      });
    })();
  }
  return appPromise;
}

export default {
  /**
   * @param {Request} request
   * @param {Record<string, any>} env
   * @returns {Promise<Response>}
   */
  async fetch(request, env) {
    return (await app(env))(request);
  },
};
