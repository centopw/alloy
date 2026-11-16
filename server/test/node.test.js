import assert from 'node:assert/strict';
import test from 'node:test';

import { createApp } from '../src/core.js';
import { memoryStore } from '../src/memory-store.js';
import { createServer } from '../src/node.js';
import { blobId, bytes } from '../support/helpers.js';

const TOKEN = 'node-token-4d2e';

test('the Node adapter serves the shared handler over real HTTP', async (t) => {
  const app = await createApp({
    store: memoryStore(),
    tokens: [{ token: TOKEN, quota: 1024 * 1024 }],
    version: '9.9.9',
  });
  const server = createServer(app);
  t.after(() => new Promise((resolve) => server.close(resolve)));
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = /** @type {import('node:net').AddressInfo} */ (server.address());
  const base = `http://127.0.0.1:${address.port}`;

  const health = await fetch(`${base}/healthz`);
  assert.equal(health.status, 200);
  assert.deepEqual(await health.json(), { ok: true, version: '9.9.9' });

  assert.equal((await fetch(`${base}/v1/head`)).status, 401);

  const id = blobId();
  const written = await fetch(`${base}/v1/blob/${id}`, {
    method: 'PUT',
    headers: { authorization: `Bearer ${TOKEN}`, 'content-type': 'application/octet-stream' },
    body: bytes('sealed'),
  });
  assert.equal(written.status, 204);

  const read = await fetch(`${base}/v1/blob/${id}`, { headers: { authorization: `Bearer ${TOKEN}` } });
  assert.equal(read.status, 200);
  assert.equal(await read.text(), 'sealed');
});
