/** @module memory-store - in-memory Store implementation for tests. */

/**
 * @typedef {import('./core.js').Store} Store
 */

/**
 * @returns {Store & { keys: () => string[] }}
 */
export function memoryStore() {
  /** @type {Map<string, Uint8Array>} */
  const data = new Map();
  return {
    async get(key) {
      const value = data.get(key);
      return value ? value.slice() : null;
    },
    async put(key, value) {
      data.set(key, value.slice());
    },
    async delete(key) {
      data.delete(key);
    },
    async listPrefix(prefix) {
      return [...data.keys()].filter((key) => key.startsWith(prefix)).sort();
    },
    keys() {
      return [...data.keys()].sort();
    },
  };
}
