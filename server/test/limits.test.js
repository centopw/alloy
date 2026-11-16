import assert from 'node:assert/strict';
import test from 'node:test';

import { createApp } from '../src/core.js';
import { memoryStore } from '../src/memory-store.js';
import { blobId, bytes, call } from '../support/helpers.js';

const TOKEN = 'limit-token-1b7a';
const MAX = 1024 * 1024;

async function makeApp() {
  const store = memoryStore();
  const app = await createApp({ store, tokens: [{ token: TOKEN, quota: 64 * 1024 * 1024 }], version: '9.9.9' });
  return { app, store };
}

test('a blob of exactly the cap is accepted', async () => {
  const { app } = await makeApp();
  const response = await call(app, `/v1/blob/${blobId()}`, {
    method: 'PUT',
    token: TOKEN,
    body: new Uint8Array(MAX),
  });
  assert.equal(response.status, 204);
});

test('an oversized blob is 413 and the server stops pulling the body', { timeout: 5000 }, async () => {
  const { app, store } = await makeApp();
  let pulled = 0;
  const chunk = new Uint8Array(64 * 1024);
  const body = new ReadableStream(
    {
      pull(controller) {
        pulled += chunk.byteLength;
        controller.enqueue(chunk);
      },
    },
    // No prefetch: every byte counted here was actually requested by the handler.
    { highWaterMark: 0 },
  );
  const response = await app(
    new Request(`https://sync.test/v1/blob/${blobId()}`, {
      method: 'PUT',
      headers: { authorization: `Bearer ${TOKEN}`, 'content-type': 'application/octet-stream' },
      body,
      // @ts-expect-error duplex is required for streaming request bodies
      duplex: 'half',
    }),
  );
  assert.equal(response.status, 413);
  assert.ok(pulled <= MAX + chunk.byteLength, `server pulled ${pulled} bytes; it must stop at the cap`);
  assert.deepEqual(store.keys(), []);
});

test('a lying Content-Length over the cap is rejected without reading the body', async () => {
  const { app, store } = await makeApp();
  let pulled = 0;
  let cancelled = false;
  const body = new ReadableStream(
    {
      pull(controller) {
        pulled += 1;
        controller.enqueue(new Uint8Array(1));
      },
      cancel() {
        cancelled = true;
      },
    },
    { highWaterMark: 0 },
  );
  const request = new Request(`https://sync.test/v1/blob/${blobId()}`, {
    method: 'PUT',
    headers: { authorization: `Bearer ${TOKEN}`, 'content-type': 'application/octet-stream' },
    body,
    // @ts-expect-error duplex is required for streaming request bodies
    duplex: 'half',
  });
  request.headers.set('content-length', String(MAX + 1));
  const response = await app(request);
  assert.equal(response.status, 413);
  assert.equal(pulled, 0, 'no chunk may be pulled for a body declared over the cap');
  assert.equal(cancelled, true, 'the request body must be cancelled');
  assert.deepEqual(store.keys(), []);
});

test('a body that exceeds the cap despite an honest small Content-Length is still 413', async () => {
  const { app, store } = await makeApp();
  const request = new Request(`https://sync.test/v1/blob/${blobId()}`, {
    method: 'PUT',
    headers: { authorization: `Bearer ${TOKEN}`, 'content-type': 'application/octet-stream' },
    body: bytes('x'.repeat(MAX + 1)),
  });
  request.headers.set('content-length', '10');
  const response = await app(request);
  assert.equal(response.status, 413);
  assert.deepEqual(store.keys(), []);
});
