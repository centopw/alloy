import { describe, expect, it } from 'vitest';

/**
 * Guard test for ADR 0014 §6: components hold no colour, radius or spacing literals.
 *
 * Scope: the `<style>` block of every shipped Svelte component under `src/lib`. Excluded:
 * - `src/lib/mock/**` — developer sample content, never part of a shipped build (the
 *   transport-selection tests pin the build-level guarantee);
 * - `src/lib/generated/**` — generated protocol types, not styles;
 * - token sources (`tokens.css`, `layout.css`) — the place where literals are allowed to live.
 *
 * Rules:
 * 1. No colour literal: `#rgb`, `#rrggbb`, `rgb()`, `rgba()`, `hsl()`, `hsla()` — including as a
 *    `var()` fallback. Use a theme token, or `color-mix()` of tokens.
 * 2. No `border-radius` length literal. `0`, percentages, `var()` and `calc()` are fine.
 * 3. Spacing (`padding`/`margin`/`gap` and their logical variants) must not repeat the spacing
 *    scale: 4, 8, 12, 16, 20, 24, 32 and 40px are `--alloy-spacing-scale-1…8`. Values of 3px or
 *    less are optical nudges and are allowed. Any other literal (control metrics such as a 10px
 *    button inset, negative hit-area expansions) must carry a `token-audit:` comment on the same
 *    line with the reason, so every deviation stays reviewable.
 */

const OPTICAL_MAX_PX = 3;
const SCALE_PX: Record<number, string> = {
  4: '1',
  8: '2',
  12: '3',
  16: '4',
  20: '5',
  24: '6',
  32: '7',
  40: '8',
};
const SPACING_PROPERTY =
  /^(padding|margin|gap)(-(top|right|bottom|left|inline|block|inline-start|inline-end|block-start|block-end))?$/;
const RADIUS_PROPERTY = /^border(-(top|bottom)-(left|right|start|end)-)?radius$/;
const LENGTH = /-?\d*\.?\d+(px|rem|em|%)/g;
const DECLARATION_START = /^\s*([a-zA-Z-]+)\s*:\s*(.*)$/;

interface Declaration {
  /** Absolute 1-based line in the .svelte file. */
  line: number;
  property: string;
  value: string;
  /** True when the declaration line carries a `token-audit:` reason (possibly inside a comment). */
  justified: boolean;
}

/** Returns each declaration of every `<style>` block, with absolute line numbers. */
function declarations(source: string): Declaration[] {
  const out: Declaration[] = [];
  const blocks = /<style[^>]*>([\s\S]*?)<\/style>/g;
  let block: RegExpExecArray | null;
  while ((block = blocks.exec(source)) !== null) {
    const contentStart = block.index + block[0].indexOf('>') + 1;
    const baseLine = source.slice(0, contentStart).split('\n').length;
    const content = block[1].replace(/\/\*[\s\S]*?\*\//g, (comment) =>
      '\n'.repeat((comment.match(/\n/g) ?? []).length)
    );
    const justifiedLines = new Set<number>();
    block[1].split('\n').forEach((raw, index) => {
      if (raw.includes('token-audit:')) justifiedLines.add(baseLine + index);
    });

    let current: Declaration | null = null;
    content.split('\n').forEach((raw, index) => {
      const line = baseLine + index;
      if (!current) {
        const start = DECLARATION_START.exec(raw);
        if (!start || start[2].trim().endsWith('{')) return;
        current = {
          line,
          property: start[1],
          value: start[2],
          justified: justifiedLines.has(line),
        };
      } else {
        current.value += ' ' + raw.trim();
      }
      if (current && current.value.includes(';')) {
        current.value = current.value.slice(0, current.value.indexOf(';'));
        out.push(current);
        current = null;
      }
    });
  }
  return out;
}

/** Removes var() and calc() bodies so only top-level literals remain. */
function stripFunctions(value: string): string {
  let previous = '';
  let current = value;
  while (current !== previous) {
    previous = current;
    current = current.replace(/var\([^()]*\)|calc\([^()]*\)/g, ' ');
  }
  return current;
}

const modules = import.meta.glob<string>('./**/*.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
});

const AUDITED = Object.entries(modules).filter(
  ([file]) => !file.includes('/mock/') && !file.includes('/generated/')
);

describe('tokens-audit: ADR 0014 token discipline', () => {
  it('finds the components to audit', () => {
    expect(AUDITED.length).toBeGreaterThan(80);
  });

  it('uses no colour literals', () => {
    const violations: string[] = [];
    for (const [file, source] of AUDITED) {
      for (const { line, property, value } of declarations(source)) {
        const colour = /#[0-9a-fA-F]{3,8}\b|\b(rgba?|hsla?)\(/.exec(value);
        if (colour) violations.push(`${file}:${line} - \`${property}\` uses \`${colour[0]}\``);
      }
    }
    expect(violations, `Colour literals found:\n${violations.join('\n')}`).toEqual([]);
  });

  it('uses no border-radius literals', () => {
    const violations: string[] = [];
    for (const [file, source] of AUDITED) {
      for (const { line, property, value } of declarations(source)) {
        if (!RADIUS_PROPERTY.test(property)) continue;
        const literal = /-?\d*\.?\d+(px|rem|em)?/.exec(stripFunctions(value));
        if (literal && Number.parseFloat(literal[0]) !== 0) {
          violations.push(`${file}:${line} - \`${property}: ${value}\``);
        }
      }
    }
    expect(violations, `Radius literals found:\n${violations.join('\n')}`).toEqual([]);
  });

  it('keeps spacing on the token scale', () => {
    const violations: string[] = [];
    for (const [file, source] of AUDITED) {
      for (const { line, property, value, justified } of declarations(source)) {
        if (!SPACING_PROPERTY.test(property)) continue;

        const stripped = stripFunctions(value);
        const lengths = stripped.match(LENGTH) ?? [];
        for (const length of lengths) {
          const amount = Math.abs(Number.parseFloat(length));
          const unit = length.replace(/^-?\d*\.?\d+/, '');
          if (unit === '%' || unit === 'em') continue; // relative to the box or the text, not the scale
          if (amount <= OPTICAL_MAX_PX) continue; // optical nudge, below the smallest step
          const scale = unit === 'px' ? SCALE_PX[amount] : undefined;
          if (scale) {
            violations.push(
              `${file}:${line} - \`${property}: ${value}\` uses ${length}; use --alloy-spacing-scale-${scale}`
            );
          } else if (!justified) {
            violations.push(
              `${file}:${line} - \`${property}: ${value}\` is off the scale and has no \`token-audit:\` reason`
            );
          }
        }
      }
    }
    expect(violations, `Spacing literals found:\n${violations.join('\n')}`).toEqual([]);
  });
});
