import assert from 'node:assert/strict';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { diskStore } from '../src/disk-store.js';
import { blobId, bytes } from '../support/helpers.js';

async function tempRoot(t) {
  const root = await mkdtemp(join(tmpdir(), 'alloy-sync-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}

test('roundtrip: put, get, overwrite, delete, prefix list', async (t) => {
  const root = await tempRoot(t);
  const store = await diskStore(root);
  const id = blobId();

  assert.equal(await store.get(`blob/${id}`), null);
  await store.put(`blob/${id}`, bytes('first'));
  assert.deepEqual(new Uint8Array((await store.get(`blob/${id}`)) ?? new Uint8Array()), bytes('first'));

  await store.put(`blob/${id}`, bytes('second'));
  assert.deepEqual(new Uint8Array((await store.get(`blob/${id}`)) ?? new Uint8Array()), bytes('second'));

  await store.put('head', bytes('head'));
  await store.put(`usage/${'a'.repeat(64)}`, bytes('11'));

  assert.deepEqual(await store.listPrefix('blob/'), [`blob/${id}`]);
  assert.deepEqual(await store.listPrefix(''), [`blob/${id}`, 'head', `usage/${'a'.repeat(64)}`].sort());

  await store.delete(`blob/${id}`);
  assert.equal(await store.get(`blob/${id}`), null);
  assert.deepEqual(await store.listPrefix('blob/'), []);

  // Delete is idempotent and the scratch directory holds no leftovers.
  await store.delete(`blob/${id}`);
  assert.deepEqual(await readdir(join(root, '.tmp')), []);
});

test('traversal and non-name keys are rejected before touching the filesystem', async (t) => {
  const root = await tempRoot(t);
  const store = await diskStore(root);
  const bad = ['../escape', 'blob/../../escape', '/absolute', 'blob//x', '', 'blob/.', '.tmp/x', 'a/b/c/d'];

  for (const key of bad) {
    await assert.rejects(() => store.put(key, bytes('x')), `put ${key}`);
    await assert.rejects(() => store.get(key), `get ${key}`);
    await assert.rejects(() => store.delete(key), `delete ${key}`);
  }

  assert.deepEqual(await readdir(root), ['.tmp']);
});
