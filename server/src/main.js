/**
 * @module main - self-hosted entry point: a directory-on-disk store served over plain
 * HTTP, meant to sit behind a TLS-terminating reverse proxy.
 *
 * Environment:
 *   SYNC_DATA_DIR    required, directory holding the opaque blobs
 *   SYNC_TOKENS      comma separated `token` or `token:quota_bytes`
 *   SYNC_TOKENS_FILE alternative to SYNC_TOKENS; one entry per line
 *   SYNC_LOG=1       access log (method, status, duration only)
 *   PORT / HOST      defaults 8080 / 0.0.0.0
 */

import { readFileSync } from 'node:fs';

import { createApp } from './core.js';
import { diskStore } from './disk-store.js';
import { createServer } from './node.js';
import { parseTokenSpec } from './tokens.js';
import { VERSION } from './version.js';

const dataDir = process.env.SYNC_DATA_DIR;
if (!dataDir) {
  console.error('SYNC_DATA_DIR is required');
  process.exit(1);
}

let spec = process.env.SYNC_TOKENS ?? '';
if (!spec && process.env.SYNC_TOKENS_FILE) {
  spec = readFileSync(process.env.SYNC_TOKENS_FILE, 'utf8');
}
const tokens = parseTokenSpec(spec);
if (tokens.length === 0) {
  console.error('configure at least one token in SYNC_TOKENS or SYNC_TOKENS_FILE');
  process.exit(1);
}

const app = await createApp({
  store: await diskStore(dataDir),
  tokens,
  version: VERSION,
  log: process.env.SYNC_LOG === '1' ? (line) => console.log(line) : null,
});

const port = Number(process.env.PORT ?? 8080);
const host = process.env.HOST ?? '0.0.0.0';
createServer(app).listen(port, host, () => {
  console.log(`alloy sync server listening on ${host}:${port}`);
});
