import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

/**
 * Guard test: ensure no emoji in UI chrome (decision 0001).
 * Allows emoji in test fixtures and ipc/mock only.
 */

const EMOJI_REGEX = /\p{Extended_Pictographic}/u;
const UI_ROOT = path.resolve(__dirname, '..', '..', 'src', 'lib');
const ALLOWED_PATHS = [
  path.resolve(UI_ROOT, 'mock'),
  path.resolve(UI_ROOT, 'ipc', 'mock.ts'),
  path.resolve(UI_ROOT, 'content', 'emoji.ts'),
  path.resolve(UI_ROOT, 'identity', 'identity.test.ts') // Tests can use emoji as test data
];

function findFiles(dir: string): string[] {
  let results: string[] = [];
  const list = fs.readdirSync(dir);
  for (const file of list) {
    const full = path.join(dir, file);
    const stat = fs.statSync(full);
    if (stat.isDirectory()) {
      results = results.concat(findFiles(full));
    } else if (file.endsWith('.ts') || file.endsWith('.svelte')) {
      results.push(full);
    }
  }
  return results;
}

function isAllowed(filePath: string): boolean {
  return ALLOWED_PATHS.some(
    (allowed) => filePath === allowed || filePath.startsWith(allowed + path.sep)
  );
}

describe('emoji-guard: no emoji icons in UI chrome', () => {
  it('should not contain emoji characters outside test/mock directories', () => {
    const allFiles = findFiles(UI_ROOT);
    const violations: Array<{ file: string; lines: Array<{ line: number; text: string }> }> = [];

    for (const file of allFiles) {
      if (isAllowed(file)) continue;

      const content = fs.readFileSync(file, 'utf8');
      const lines = content.split('\n');
      const emojiLines: Array<{ line: number; text: string }> = [];

      lines.forEach((text, idx) => {
        if (EMOJI_REGEX.test(text)) {
          emojiLines.push({ line: idx + 1, text: text.trim().substring(0, 80) });
        }
      });

      if (emojiLines.length > 0) {
        const relPath = path.relative(UI_ROOT, file);
        violations.push({ file: relPath, lines: emojiLines });
      }
    }

    expect(
      violations,
      `Found ${violations.length} file(s) with emoji. Decision 0001: no decorative icons.\n${violations
        .map(
          (v) =>
            `${v.file}:\n${v.lines.map((l) => `  L${l.line}: ${l.text}`).join('\n')}`
        )
        .join('\n')}`
    ).toEqual([]);
  });
});
