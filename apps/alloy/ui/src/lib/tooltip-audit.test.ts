import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

/**
 * Guard test: every icon-only control must expose an accessible name and a tooltip
 * source. The TooltipLayer derives tooltips from aria-label, title, or data-tooltip,
 * so icon-only buttons need one of those on the opening tag.
 *
 * A control is icon-only when its body contains an <Icon> or <Avatar> component,
 * no other capitalised component tag, and no literal text.
 *
 * Entries are "relative/path.svelte:line". Keep empty unless a reviewed exception exists.
 */
const ALLOWLIST: string[] = [];

const UI_ROOT = path.resolve(__dirname, '..', '..', 'src', 'lib');
const OPEN_TAG = /<(button|Button)(?=[\s>/])/g;
const LABEL_ATTR = /\s(?:aria-label|ariaLabel|title|data-tooltip|tooltip)=/;
const COMPONENT_TAG = /<([A-Z][A-Za-z0-9]*)[\s/>]/g;
const ICON_COMPONENTS: Record<string, true> = { Icon: true, Avatar: true };

function findSvelteFiles(dir: string): string[] {
  let results: string[] = [];
  for (const entry of fs.readdirSync(dir)) {
    const full = path.join(dir, entry);
    if (fs.statSync(full).isDirectory()) {
      results = results.concat(findSvelteFiles(full));
    } else if (entry.endsWith('.svelte') && !entry.includes('.test.')) {
      results.push(full);
    }
  }
  return results;
}

/** Returns the index of the '>' closing the opening tag, skipping quoted and braced content. */
function findOpeningTagEnd(src: string, start: number): number {
  let depth = 0;
  let quote: string | null = null;
  for (let i = start; i < src.length; i++) {
    const ch = src[i];
    if (quote) {
      if (ch === quote) quote = null;
      continue;
    }
    if (depth === 0 && (ch === '"' || ch === "'")) {
      quote = ch;
    } else if (ch === '{') {
      depth++;
    } else if (ch === '}') {
      depth--;
    } else if (ch === '>' && depth === 0) {
      return i;
    }
  }
  return -1;
}

/**
 * Icon-only: body has an Icon or Avatar component, no other capitalised component
 * tag, and no literal text outside tags and control blocks.
 */
function isIconOnly(body: string): boolean {
  const components = [...body.matchAll(COMPONENT_TAG)].map((m) => m[1]);
  const hasIcon = components.some((name) => ICON_COMPONENTS[name] === true);
  const hasOtherComponent = components.some((name) => ICON_COMPONENTS[name] !== true);
  const literalText = body
    .replace(/<[^>]*>/g, '')
    .replace(/\{[#:/@][^}]*\}/g, '')
    .trim();
  return hasIcon && !hasOtherComponent && literalText.length === 0;
}

function findUnlabelledIconControls(file: string): Array<{ line: number; tag: string }> {
  const src = fs.readFileSync(file, 'utf8');
  const found: Array<{ line: number; tag: string }> = [];
  OPEN_TAG.lastIndex = 0;

  let match: RegExpExecArray | null;
  while ((match = OPEN_TAG.exec(src)) !== null) {
    const start = match.index;
    const name = match[1];
    const end = findOpeningTagEnd(src, start + match[0].length);
    if (end < 0) continue;

    const opening = src.slice(start, end + 1);
    // Spread attributes can carry a label we cannot see statically; skip them.
    if (opening.includes('{...')) continue;

    let body = '';
    if (src[end - 1] !== '/') {
      const closeIndex = src.indexOf(`</${name}>`, end + 1);
      if (closeIndex < 0) continue;
      body = src.slice(end + 1, closeIndex);
    }

    if (!isIconOnly(body)) continue;
    if (LABEL_ATTR.test(opening)) continue;

    found.push({
      line: src.slice(0, start).split('\n').length,
      tag: opening.split('\n')[0].trim().slice(0, 80),
    });
  }
  return found;
}

describe('tooltip-audit: icon-only controls expose a name and tooltip', () => {
  it('every icon-only <button> and <Button> has aria-label, title, or tooltip', () => {
    const violations: string[] = [];

    for (const file of findSvelteFiles(UI_ROOT)) {
      const rel = path.relative(UI_ROOT, file);
      for (const hit of findUnlabelledIconControls(file)) {
        const key = `${rel}:${hit.line}`;
        if (ALLOWLIST.includes(key)) continue;
        violations.push(`${key}  ${hit.tag}`);
      }
    }

    expect(
      violations,
      `Found ${violations.length} icon-only control(s) without aria-label, title, or tooltip.\n${violations.join('\n')}`
    ).toEqual([]);
  });
});
