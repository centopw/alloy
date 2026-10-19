import { describe, expect, it } from 'vitest';

/**
 * Guard tests: dropdowns and text form fields must use design-system common controls
 * (Select, TextField, TextArea) rather than raw browser controls.
 *
 * Fails on:
 * - Any <select> outside Select.svelte
 * - Any <textarea> outside ALLOWLIST
 * - Any <input> outside ALLOWLIST unless its type is a literal non-text type (see NON_TEXT_INPUT_TYPES).
 *   A missing or dynamic type (type={...}) cannot be proven non-text, so it fails.
 */

const ALLOWLIST: Record<string, true> = {
  'TextField.svelte': true,
  'TextArea.svelte': true,
  'Composer.svelte': true,
  'CommandPalette.svelte': true,
  'Sidebar.svelte': true,
};

const NON_TEXT_INPUT_TYPES: Record<string, true> = {
  checkbox: true,
  radio: true,
  file: true,
  range: true,
  hidden: true,
  button: true,
  submit: true,
  reset: true,
  image: true,
};

/** Returns the index of the '>' closing the opening tag, skipping quoted and braced content. */
function findOpeningTagEnd(src: string, start: number): number {
  let depth = 0;
  let quote: string | null = null;
  for (let i = start; i < src.length; i++) {
    const ch = src[i];
    if (quote) {
      if (ch === quote && src[i - 1] !== '\\') quote = null;
    } else if (ch === '"' || ch === "'") {
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

describe('controls-audit: form fields use common design system controls', () => {
  const modules = import.meta.glob<string>('./**/*.svelte', {
    query: '?raw',
    import: 'default',
    eager: true,
  });

  it('prohibits raw select outside Select.svelte', () => {
    const violations: string[] = [];

    for (const [file, content] of Object.entries(modules)) {
      const base = file.split('/').pop() ?? file;
      if (base === 'Select.svelte') continue;

      const regex = /<select(?=[\s>/])/g;
      let match: RegExpExecArray | null;
      while ((match = regex.exec(content)) !== null) {
        const line = content.slice(0, match.index).split('\n').length;
        violations.push(`${file}:${line} - raw <select> used instead of Select`);
      }
    }

    expect(
      violations,
      `Found unauthorized <select> tags:\n${violations.join('\n')}`,
    ).toEqual([]);
  });

  it('prohibits raw textarea outside allowlist', () => {
    const violations: string[] = [];

    for (const [file, content] of Object.entries(modules)) {
      const base = file.split('/').pop() ?? file;
      if (ALLOWLIST[base]) continue;

      const regex = /<textarea(?=[\s>/])/g;
      let match: RegExpExecArray | null;
      while ((match = regex.exec(content)) !== null) {
        const line = content.slice(0, match.index).split('\n').length;
        violations.push(`${file}:${line} - raw <textarea> used instead of TextArea`);
      }
    }

    expect(
      violations,
      `Found unauthorized <textarea> tags:\n${violations.join('\n')}`,
    ).toEqual([]);
  });

  it('prohibits raw text-like inputs outside allowlist', () => {
    const violations: string[] = [];

    for (const [file, content] of Object.entries(modules)) {
      const base = file.split('/').pop() ?? file;
      if (ALLOWLIST[base]) continue;

      const regex = /<input(?=[\s>/])/g;
      let match: RegExpExecArray | null;
      while ((match = regex.exec(content)) !== null) {
        const end = findOpeningTagEnd(content, match.index);
        const tag = end !== -1 ? content.slice(match.index, end + 1) : match[0];
        const line = content.slice(0, match.index).split('\n').length;

        const typeMatch = tag.match(/\stype=(?:"([^"]*)"|'([^']*)'|([^\s>]+))/i);
        const type = typeMatch
          ? (typeMatch[1] ?? typeMatch[2] ?? typeMatch[3]).toLowerCase()
          : null;

        // A missing or non-literal type cannot be proven non-text, so only known non-text types pass.
        if (!type || !NON_TEXT_INPUT_TYPES[type]) {
          violations.push(
            `${file}:${line} - raw <input${type ? ` type="${type}"` : ''}> used instead of TextField`,
          );
        }
      }
    }

    expect(
      violations,
      `Found unauthorized <input> tags:\n${violations.join('\n')}`,
    ).toEqual([]);
  });
});
