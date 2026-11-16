/**
 * @module kv-store - Cloudflare Workers KV adapter for the hosted mode.
 *
 * Same minimal Store interface; KV is eventually consistent by design, so a CAS
 * protects against a stalled writer but not against KV's own replication lag.
 */

/**
 * @typedef {import('./core.js').Store} Store
 * @typedef {object} KVNamespace
 * @property {(key: string, type?: 'text' | 'arraybuffer') => Promise<ArrayBuffer | string | null>} get
 * @property {(key: string, value: Uint8Array) => Promise<void>} put
 * @property {(key: string) => Promise<void>} delete
 * @property {(options: { prefix?: string, cursor?: string }) => Promise<{ keys: { name: string }[], list_complete: boolean, cursor?: string }>} list
 */

/**
 * @param {KVNamespace} namespace
 * @returns {Store}
 */
export function kvStore(namespace) {
  return {
    async get(key) {
      const value = await namespace.get(key, 'arraybuffer');
      return value === null ? null : new Uint8Array(value);
    },
    async put(key, value) {
      await namespace.put(key, value);
    },
    async delete(key) {
      await namespace.delete(key);
    },
    async listPrefix(prefix) {
      /** @type {string[]} */
      const keys = [];
      /** @type {string | undefined} */
      let cursor;
      for (;;) {
        const page = await namespace.list({ prefix, cursor });
        for (const entry of page.keys) keys.push(entry.name);
        if (page.list_complete) break;
        cursor = page.cursor;
      }
      return keys.sort();
    },
  };
}
