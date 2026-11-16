import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import { DEFAULT_QUOTA_BYTES, parseTokenSpec } from '../src/tokens.js';
import { VERSION } from '../src/version.js';

const manifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));

test('package.json and the served version agree, and there are no runtime dependencies', () => {
  assert.equal(VERSION, manifest.version);
  assert.equal(manifest.type, 'module');
  assert.deepEqual(Object.keys(manifest.dependencies ?? {}), []);
});

test('token spec parsing', () => {
  assert.deepEqual(parseTokenSpec('one,two:1024'), [
    { token: 'one', quota: DEFAULT_QUOTA_BYTES },
    { token: 'two', quota: 1024 },
  ]);
  assert.deepEqual(parseTokenSpec('a\n# comment\n\nb:99\n'), [
    { token: 'a', quota: DEFAULT_QUOTA_BYTES },
    { token: 'b', quota: 99 },
  ]);
  assert.deepEqual(parseTokenSpec(''), []);
  assert.deepEqual(parseTokenSpec(undefined), []);
  // A non-numeric suffix is part of the token, not a quota.
  assert.deepEqual(parseTokenSpec('token:nan'), [{ token: 'token:nan', quota: DEFAULT_QUOTA_BYTES }]);
});
