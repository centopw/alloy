import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';

import { createApp } from '../src/core.js';
import { memoryStore } from '../src/memory-store.js';
import { blobId, bytes, call } from '../support/helpers.js';

const TOKEN = 'test-token-9f3c';

/** @param {{ quota?: number, log?: ((line: string) => void) | null }} [options] */
async function makeApp({ quota = 1024 * 1024, log = null } = {}) {
  const store = memoryStore();
  const app = await createApp({ store, tokens: [{ token: TOKEN, quota }], version: '9.9.9', log });
  return { app, store };
}

test('healthz needs no token and exposes only ok and version', async () => {
  const { app } = await makeApp();
  const response = await call(app, '/healthz');
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { ok: true, version: '9.9.9' });
  assert.equal((await call(app, '/healthz', { method: 'POST' })).status, 405);
});

test('missing, malformed and wrong tokens are 401', async () => {
  const { app } = await makeApp();
  assert.equal((await call(app, '/v1/head')).status, 401);
  assert.equal((await call(app, '/v1/head', { headers: { authorization: 'Basic abc' } })).status, 401);
  assert.equal((await call(app, '/v1/head', { token: 'wrong' })).status, 401);
  assert.equal((await call(app, '/v1/head', { token: TOKEN })).status, 404);
});

test('head create, read, and compare-and-swap', async () => {
  const { app } = await makeApp();

  const created = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('head-v1'),
    headers: { 'if-none-match': '*' },
  });
  assert.equal(created.status, 204);
  const firstEtag = created.headers.get('etag');
  assert.match(firstEtag ?? '', /^"[0-9a-f]{64}"$/);

  const read = await call(app, '/v1/head', { token: TOKEN });
  assert.equal(read.status, 200);
  assert.equal(read.headers.get('etag'), firstEtag);
  assert.equal(await read.text(), 'head-v1');

  const replaced = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('head-v2'),
    headers: { 'if-match': firstEtag },
  });
  assert.equal(replaced.status, 204);

  const stale = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('head-stale'),
    headers: { 'if-match': firstEtag },
  });
  assert.equal(stale.status, 409);
  assert.equal(stale.headers.get('etag'), replaced.headers.get('etag'));
  assert.equal(await stale.text(), 'head-v2');

  const after = await call(app, '/v1/head', { token: TOKEN });
  assert.equal(await after.text(), 'head-v2');
});

test('a stale write never lands', async () => {
  const { app, store } = await makeApp();
  const created = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('winner'),
    headers: { 'if-none-match': '*' },
  });
  const etag = created.headers.get('etag');
  await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('second'),
    headers: { 'if-match': etag },
  });
  const rejected = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('loser'),
    headers: { 'if-match': etag },
  });
  assert.equal(rejected.status, 409);
  const stored = await store.get('head');
  assert.equal(new TextDecoder().decode(stored ?? new Uint8Array()), 'second');
});

test('precondition rules: 428 without any, 412 for create-only on an existing head', async () => {
  const { app } = await makeApp();
  assert.equal((await call(app, '/v1/head', { method: 'PUT', token: TOKEN, body: bytes('x') })).status, 428);
  await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('x'),
    headers: { 'if-none-match': '*' },
  });
  const again = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('y'),
    headers: { 'if-none-match': '*' },
  });
  assert.equal(again.status, 412);
});

test('escrow and roster use the same CAS path', async () => {
  const { app } = await makeApp();
  for (const resource of ['/v1/escrow', '/v1/roster']) {
    assert.equal((await call(app, resource, { token: TOKEN })).status, 404);
    const created = await call(app, resource, {
      method: 'PUT',
      token: TOKEN,
      body: bytes(`sealed-${resource}`),
      headers: { 'if-none-match': '*' },
    });
    assert.equal(created.status, 204);
    const read = await call(app, resource, { token: TOKEN });
    assert.equal(read.status, 200);
    assert.equal(await read.text(), `sealed-${resource}`);
  }
});

test('blob roundtrip is opaque bytes under an opaque id', async () => {
  const { app, store } = await makeApp();
  const id = blobId();
  const payload = crypto.getRandomValues(new Uint8Array(4096));

  const written = await call(app, `/v1/blob/${id}`, { method: 'PUT', token: TOKEN, body: payload });
  assert.equal(written.status, 204);

  const read = await call(app, `/v1/blob/${id}`, { token: TOKEN });
  assert.equal(read.status, 200);
  assert.equal(read.headers.get('content-type'), 'application/octet-stream');
  assert.deepEqual(new Uint8Array(await read.arrayBuffer()), payload);

  assert.equal((await call(app, `/v1/blob/${blobId()}`, { token: TOKEN })).status, 404);
  // Nothing about the body is interpreted: the stored key is the id, the value is the payload.
  assert.deepEqual(await store.listPrefix('blob/'), [`blob/${id}`]);
});

test('escaped and traversal-looking ids are rejected before storage', async () => {
  const { app, store } = await makeApp();
  // The URL parser folds a literal ".." segment away, so those requests land on an
  // unknown route (404); encoded or multi-segment ids reach the router and fail the
  // shape check (400). Either way nothing is stored and no id is interpreted.
  const ids = [
    '..',
    '.',
    '%2e%2e',
    '%2e%2e%2f%2e%2e%2fetc%2fpasswd',
    'a/b',
    'A'.repeat(42),
    'A'.repeat(44),
    'A'.repeat(40) + '!',
  ];
  for (const id of ids) {
    const response = await call(app, `/v1/blob/${id}`, { token: TOKEN });
    assert.ok([400, 404].includes(response.status), `id ${id} must be rejected, got ${response.status}`);
  }
  assert.equal((await call(app, '/v1/blob/', { token: TOKEN })).status, 400);
  assert.deepEqual(store.keys(), []);
});

test('content type must be application/octet-stream', async () => {
  const { app, store } = await makeApp();
  const id = blobId();
  const response = await call(app, `/v1/blob/${id}`, {
    method: 'PUT',
    token: TOKEN,
    body: bytes('{"plain":true}'),
    headers: { 'content-type': 'application/json' },
  });
  assert.equal(response.status, 415);
  assert.deepEqual(store.keys(), []);
});

test('delete removes a blob, needs auth, and a second delete is 404', async () => {
  const { app, store } = await makeApp();
  const id = blobId();
  const payload = new TextEncoder().encode('ciphertext');

  assert.equal((await call(app, `/v1/blob/${id}`, { method: 'DELETE' })).status, 401);
  assert.equal((await call(app, `/v1/blob/${id}`, { method: 'DELETE', token: 'wrong' })).status, 401);
  // Never written: 404, and nothing is invented.
  assert.equal((await call(app, `/v1/blob/${id}`, { method: 'DELETE', token: TOKEN })).status, 404);

  await call(app, `/v1/blob/${id}`, { method: 'PUT', token: TOKEN, body: payload });
  assert.deepEqual(await store.listPrefix('blob/'), [`blob/${id}`]);

  assert.equal((await call(app, `/v1/blob/${id}`, { method: 'DELETE', token: TOKEN })).status, 204);
  assert.equal((await call(app, `/v1/blob/${id}`, { token: TOKEN })).status, 404);
  assert.deepEqual(await store.listPrefix('blob/'), []);
  // Idempotent in effect, a 404 in answer: the second delete finds nothing to remove.
  assert.equal((await call(app, `/v1/blob/${id}`, { method: 'DELETE', token: TOKEN })).status, 404);
});

test('delete rejects malformed ids and never touches a key outside blob/', async () => {
  const { app, store } = await makeApp();
  const head = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('head'),
    headers: { 'if-none-match': '*' },
  });
  assert.equal(head.status, 204);
  const kept = store.keys();

  const ids = [
    '..',
    '.',
    '%2e%2e',
    '%2e%2e%2f%2e%2e%2fhead',
    'a/b',
    'A'.repeat(42),
    'A'.repeat(44),
    'A'.repeat(40) + '!',
  ];
  for (const id of ids) {
    const response = await call(app, `/v1/blob/${id}`, { method: 'DELETE', token: TOKEN });
    assert.ok([400, 404].includes(response.status), `id ${id} must be rejected, got ${response.status}`);
  }
  assert.equal((await call(app, '/v1/blob/', { method: 'DELETE', token: TOKEN })).status, 400);
  assert.deepEqual(store.keys(), kept, 'no key outside the blob prefix was deleted');
  assert.equal((await call(app, '/v1/head', { token: TOKEN })).status, 200);
});

test('delete does not refund the token quota', async () => {
  const { app, store } = await makeApp({ quota: 100 });
  const digest = createHash('sha256').update(TOKEN).digest('hex');
  const usage = `usage/${digest}`;
  const id = blobId();

  assert.equal(
    (await call(app, `/v1/blob/${id}`, { method: 'PUT', token: TOKEN, body: new Uint8Array(10) })).status,
    204,
  );
  assert.equal(new TextDecoder().decode(await store.get(usage)), '10');
  assert.equal((await call(app, `/v1/blob/${id}`, { method: 'DELETE', token: TOKEN })).status, 204);
  assert.equal(new TextDecoder().decode(await store.get(usage)), '10', 'deleting is not a refund');
});

test('concurrent PUT of the same head has exactly one winner', async () => {
  const { app } = await makeApp();
  const created = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: bytes('seed'),
    headers: { 'if-none-match': '*' },
  });
  assert.equal(created.status, 204);

  for (let round = 0; round < 10; round += 1) {
    const current = await call(app, '/v1/head', { token: TOKEN });
    const etag = current.headers.get('etag');
    const bodies = [bytes(`a-${round}`), bytes(`b-${round}`)];
    const responses = await Promise.all(
      bodies.map((body) =>
        call(app, '/v1/head', { method: 'PUT', token: TOKEN, body, headers: { 'if-match': etag } }),
      ),
    );
    const statuses = responses.map((response) => response.status).sort();
    assert.deepEqual(statuses, [204, 409], `round ${round}`);

    const winner = responses[0].status === 204 ? bodies[0] : bodies[1];
    const settled = new Uint8Array(await (await call(app, '/v1/head', { token: TOKEN })).arrayBuffer());
    assert.deepEqual(settled, winner, `round ${round}: the winning write is the stored head`);
  }
});

test('per-token quota rejects with 429 before the payload is stored', async () => {
  const { app, store } = await makeApp({ quota: 20 });
  const first = blobId();
  const second = blobId();

  assert.equal((await call(app, `/v1/blob/${first}`, { method: 'PUT', token: TOKEN, body: new Uint8Array(10) })).status, 204);
  assert.equal((await call(app, `/v1/blob/${second}`, { method: 'PUT', token: TOKEN, body: new Uint8Array(11) })).status, 429);
  assert.deepEqual(await store.listPrefix('blob/'), [`blob/${first}`]);

  // Head writes count against the same quota.
  const head = await call(app, '/v1/head', {
    method: 'PUT',
    token: TOKEN,
    body: new Uint8Array(11),
    headers: { 'if-none-match': '*' },
  });
  assert.equal(head.status, 429);
});

test('logging is off by default and never names a path, id or token when on', async () => {
  const quiet = await makeApp();
  const id = blobId();
  await call(quiet.app, `/v1/blob/${id}`, { method: 'PUT', token: TOKEN, body: bytes('x') });

  /** @type {string[]} */
  const lines = [];
  const loud = await makeApp({ log: (line) => lines.push(line) });
  const logged = await call(loud.app, `/v1/blob/${id}`, { method: 'PUT', token: TOKEN, body: bytes('x') });
  assert.equal(logged.status, 204);
  assert.equal(lines.length, 1);
  assert.match(lines[0], /^PUT 204 \d+ms$/);
  assert.ok(!lines[0].includes(id), 'log must not contain the blob id');
  assert.ok(!lines[0].includes(TOKEN), 'log must not contain the token');
  assert.ok(!lines[0].includes('/v1/'), 'log must not contain a path');
});
