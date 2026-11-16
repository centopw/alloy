import assert from 'node:assert/strict';
import test from 'node:test';

import worker from '../src/worker.js';
import { blobId, bytes } from '../support/helpers.js';

const TOKEN = 'worker-token-a71f';

function fakeKv() {
  /** @type {Map<string, Uint8Array>} */
  const data = new Map();
  return {
    async get(key, type) {
      const value = data.get(key);
      if (value === undefined) return null;
      return type === 'arraybuffer' ? value.slice().buffer : new TextDecoder().decode(value);
    },
    async put(key, value) {
      data.set(key, value instanceof Uint8Array ? value.slice() : new Uint8Array(value));
    },
    async delete(key) {
      data.delete(key);
    },
    async list({ prefix = '' } = {}) {
      const keys = [...data.keys()].filter((key) => key.startsWith(prefix)).sort().map((name) => ({ name }));
      return { keys, list_complete: true };
    },
  };
}

test('the Worker entry routes through the shared handler', async () => {
  const env = { SYNC_KV: fakeKv(), SYNC_TOKENS: `${TOKEN}:1048576` };

  const health = await worker.fetch(new Request('https://sync.test/healthz'), env);
  assert.equal(health.status, 200);
  assert.deepEqual(await health.json(), { ok: true, version: '0.1.0' });

  assert.equal((await worker.fetch(new Request('https://sync.test/v1/head'), env)).status, 401);

  const id = blobId();
  const created = await worker.fetch(
    new Request('https://sync.test/v1/head', {
      method: 'PUT',
      headers: {
        authorization: `Bearer ${TOKEN}`,
        'content-type': 'application/octet-stream',
        'if-none-match': '*',
      },
      body: bytes('head-v1'),
    }),
    env,
  );
  assert.equal(created.status, 204);

  const head = await worker.fetch(
    new Request('https://sync.test/v1/head', { headers: { authorization: `Bearer ${TOKEN}` } }),
    env,
  );
  assert.equal(await head.text(), 'head-v1');

  const blob = await worker.fetch(
    new Request(`https://sync.test/v1/blob/${id}`, {
      method: 'PUT',
      headers: { authorization: `Bearer ${TOKEN}`, 'content-type': 'application/octet-stream' },
      body: bytes('opaque'),
    }),
    env,
  );
  assert.equal(blob.status, 204);

  const read = await worker.fetch(
    new Request(`https://sync.test/v1/blob/${id}`, { headers: { authorization: `Bearer ${TOKEN}` } }),
    env,
  );
  assert.equal(read.status, 200);
  assert.equal(await read.text(), 'opaque');

  const leftover = await worker.fetch(
    new Request('https://sync.test/v1/blob/not-an-id', { headers: { authorization: `Bearer ${TOKEN}` } }),
    env,
  );
  assert.equal(leftover.status, 400);
});
