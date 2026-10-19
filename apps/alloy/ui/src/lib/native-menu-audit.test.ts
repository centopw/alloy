import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

/**
 * Guard test: the OS-native context menu must never appear, and no component may open an ad-hoc
 * menu. Every contextmenu trigger routes through the shared action (`use:contextmenu`) or through
 * `contextMenu.open`. Only the shared plumbing may register raw `contextmenu` listeners.
 *
 * Entries are "relative/path.svelte:line". Keep empty unless a reviewed exception exists.
 */
const HANDLER_ALLOWLIST: string[] = [];

/** Files that own the contextmenu plumbing and may register raw `contextmenu` listeners. */
const LISTENER_FILES: string[] = ['lib/platform/contextmenu.ts', 'lib/platform/nativeMenu.ts'];

const SRC_ROOT = path.resolve(__dirname, '..');
const HANDLER_ATTR = /\b(?:oncontextmenu|on:contextmenu)=/g;
const RAW_LISTENER = /addEventListener\(\s*['"]contextmenu['"]/g;

function findSources(dir: string): string[] {
  let results: string[] = [];
  for (const entry of fs.readdirSync(dir)) {
    const full = path.join(dir, entry);
    if (fs.statSync(full).isDirectory()) {
      results = results.concat(findSources(full));
    } else if ((entry.endsWith('.svelte') || entry.endsWith('.ts')) && !entry.includes('.test.')) {
      results.push(full);
    }
  }
  return results;
}

/** Attribute value starting at `start`: a quoted string or a balanced `{...}` expression. */
function attributeValue(src: string, start: number): string {
  const open = src[start];
  if (open === '"' || open === "'") {
    return src.slice(start + 1, src.indexOf(open, start + 1));
  }
  let depth = 0;
  for (let i = start; i < src.length; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}' && --depth === 0) return src.slice(start, i + 1);
  }
  return src.slice(start);
}

function lineOf(src: string, index: number): number {
  return src.slice(0, index).split('\n').length;
}

const sources = findSources(SRC_ROOT).map((file) => ({
  rel: path.relative(SRC_ROOT, file).split(path.sep).join('/'),
  src: fs.readFileSync(file, 'utf8'),
}));

describe('native context menu audit', () => {
  it('every contextmenu handler opens our menu through contextMenu.open', () => {
    const violations: string[] = [];
    for (const { rel, src } of sources) {
      for (const m of src.matchAll(HANDLER_ATTR)) {
        const at = m.index ?? 0;
        const key = `${rel}:${lineOf(src, at)}`;
        if (HANDLER_ALLOWLIST.includes(key)) continue;
        if (attributeValue(src, at + m[0].length).includes('contextMenu.open(')) continue;
        violations.push(key);
      }
    }
    expect(violations, 'ad-hoc contextmenu handlers; use `use:contextmenu` instead').toEqual([]);
  });

  it('only the shared plumbing registers raw contextmenu listeners', () => {
    const violations: string[] = [];
    for (const { rel, src } of sources) {
      if (LISTENER_FILES.includes(rel)) continue;
      for (const m of src.matchAll(RAW_LISTENER)) {
        violations.push(`${rel}:${lineOf(src, m.index ?? 0)}`);
      }
    }
    expect(violations, 'raw contextmenu listeners; route through the guard or the action').toEqual([]);
  });

  it('the root layout installs the native menu guard', () => {
    const layout = fs.readFileSync(path.join(SRC_ROOT, 'routes', '+layout.svelte'), 'utf8');
    expect(layout).toContain('installNativeMenuGuard(document)');
  });
});
