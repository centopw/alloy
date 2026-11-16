/**
 * @module disk-store - directory-on-disk Store for self-hosting.
 *
 * Key -> path mapping is validated and cannot escape the root: every segment must be a
 * plain name. Writes go to `root/.tmp` and are renamed into place, so a reader never
 * sees a half-written blob.
 */

import { randomBytes } from 'node:crypto';
import { mkdir, readdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join, relative, sep } from 'node:path';

const SEGMENT = /^[A-Za-z0-9._-]+$/;
const SCRATCH = '.tmp';

/**
 * @typedef {import('./core.js').Store} Store
 */

/**
 * @param {string} root
 * @param {string} key
 * @returns {string}
 */
function keyToPath(root, key) {
  const segments = String(key).split('/');
  if (segments.length === 0 || segments.length > 3) throw new Error(`invalid store key: ${key}`);
  for (const segment of segments) {
    if (!SEGMENT.test(segment) || segment === '.' || segment === '..') {
      throw new Error(`invalid store key: ${key}`);
    }
  }
  if (segments[0] === SCRATCH) throw new Error(`invalid store key: ${key}`);
  return join(root, ...segments);
}

/**
 * @param {string} root
 * @param {string} dir
 * @param {string[]} out
 * @returns {Promise<void>}
 */
async function walk(root, dir, out) {
  let entries;
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch (error) {
    if (/** @type {NodeJS.ErrnoException} */ (error).code === 'ENOENT') return;
    throw error;
  }
  for (const entry of entries) {
    if (entry.name === SCRATCH) continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) await walk(root, full, out);
    else if (entry.isFile()) out.push(relative(root, full).split(sep).join('/'));
  }
}

/**
 * @param {string} root
 * @returns {Promise<Store>}
 */
export async function diskStore(root) {
  await mkdir(join(root, SCRATCH), { recursive: true });

  return {
    async get(key) {
      try {
        return new Uint8Array(await readFile(keyToPath(root, key)));
      } catch (error) {
        if (/** @type {NodeJS.ErrnoException} */ (error).code === 'ENOENT') return null;
        throw error;
      }
    },
    async put(key, value) {
      const target = keyToPath(root, key);
      const scratch = join(root, SCRATCH, randomBytes(12).toString('hex'));
      await mkdir(dirname(target), { recursive: true });
      await writeFile(scratch, value);
      await rename(scratch, target);
    },
    async delete(key) {
      await rm(keyToPath(root, key), { force: true });
    },
    async listPrefix(prefix) {
      /** @type {string[]} */
      const keys = [];
      await walk(root, root, keys);
      return keys.filter((key) => key.startsWith(prefix)).sort();
    },
  };
}
