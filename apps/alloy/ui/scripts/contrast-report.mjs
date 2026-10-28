#!/usr/bin/env node
/**
 * ADR 0014 contrast re-check: secondary text and hairlines over the brightest bloom.
 *
 * The chrome and conversation panes are translucent, so contrast depends on what drifts behind
 * them. This measures the worst case: a pane sitting on the centre of the brightest aurora bloom
 * (accent-primary at full strength over the canvas), which is the brightest point the backdrop
 * filter can hand to the pane.
 *
 *   node scripts/contrast-report.mjs          # markdown table on stdout
 *   node scripts/contrast-report.mjs --check  # exit 1 if secondary text drops below 4.5:1
 *
 * The blur and saturate(140-160%) filters shift the bloom in practice; this is a bound, not a
 * screenshot. Recorded in docs/decisions/0014-contrast.md.
 */
import fs from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const themesDir = resolve(__dirname, '../../../../packages/themes');
const check = process.argv.includes('--check');

const THEMES = [
  { id: 'alloy-light', label: 'Light' },
  { id: 'alloy-dark', label: 'Dark' },
  { id: 'alloy-amoled', label: 'AMOLED' },
];

/** @param {string} hex */
function rgb(hex) {
  let value = hex.replace('#', '');
  if (value.length === 3) value = value.split('').map((c) => c + c).join('');
  if (value.length === 8) value = value.slice(0, 6);
  return {
    r: Number.parseInt(value.slice(0, 2), 16),
    g: Number.parseInt(value.slice(2, 4), 16),
    b: Number.parseInt(value.slice(4, 6), 16),
  };
}

/** @param {{r:number,g:number,b:number}} colour @param {number} alpha @param {{r:number,g:number,b:number}} behind */
function over(colour, alpha, behind) {
  return {
    r: colour.r * alpha + behind.r * (1 - alpha),
    g: colour.g * alpha + behind.g * (1 - alpha),
    b: colour.b * alpha + behind.b * (1 - alpha),
  };
}

/** @param {{r:number,g:number,b:number}} colour */
function luminance({ r, g, b }) {
  const channel = (value) => {
    const c = value / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** @param {{r:number,g:number,b:number}} a @param {{r:number,g:number,b:number}} b */
function ratio(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function fmt(value) {
  return value.toFixed(2);
}

const rows = [];
const failures = [];

/** Deep-merges a theme's tokens over the tokens of the theme it `extends`. */
function loadTheme(id, seen = new Set()) {
  if (seen.has(id)) throw new Error(`theme ${id} extends itself`);
  seen.add(id);
  const theme = JSON.parse(fs.readFileSync(resolve(themesDir, `${id}.json`), 'utf8'));
  if (!theme.extends) return theme;
  const base = loadTheme(theme.extends, seen);
  const merge = (target, source) => {
    for (const [key, value] of Object.entries(source)) {
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        merge((target[key] ??= {}), value);
      } else {
        target[key] = value;
      }
    }
    return target;
  };
  return { ...theme, tokens: merge(structuredClone(base.tokens), theme.tokens) };
}

for (const { id, label } of THEMES) {
  const theme = loadTheme(id);
  const { color } = theme.tokens;
  const light = theme.mode === 'light';

  const canvas = rgb(color.surface.canvas);
  const panel = rgb(color.surface.panel);
  const text = {
    primary: rgb(color.text.primary),
    secondary: rgb(color.text.secondary),
    muted: rgb(color.text.muted),
  };

  // Worst case: the brightest bloom centre (ADR 0014 §1) hands the pane its brightest backdrop.
  const bloom = over(rgb(color.accent.primary), light ? 0.82 : 0.42, canvas);
  const chrome = over(panel, light ? 0.86 : 0.8, bloom);
  const chromeStrong = over(panel, light ? 0.9 : 0.86, bloom);
  const chat = over(canvas, light ? 0.92 : 0.84, bloom);
  // A floating dialog sits 64% panel over the conversation, not directly over the bloom.
  const glass = over(panel, 0.64, chat);
  // `--alloy-chat-rim` is text-primary at 8% over the conversation surface.
  const rim = over(text.primary, 0.08, chat);

  const surfaces = [
    ['chrome', chrome],
    ['chrome-strong', chromeStrong],
    ['conversation', chat],
    ['floating glass', glass],
  ];
  for (const [surfaceName, surface] of surfaces) {
    for (const [name, colour] of Object.entries(text)) {
      const value = ratio(colour, surface);
      rows.push([label, `${name} on ${surfaceName}`, fmt(value)]);
      if (name !== 'primary' && value < 4.5) {
        failures.push(`${label}: ${name} on ${surfaceName} is ${fmt(value)}:1 (needs 4.5:1)`);
      }
    }
  }

  const rimValue = ratio(rim, chat);
  rows.push([label, 'hairline (chat-rim) on conversation', fmt(rimValue)]);
}

console.log('| Theme | Pair | Ratio |');
console.log('|---|---|---|');
for (const row of rows) console.log(`| ${row.join(' | ')} |`);

if (check) {
  if (failures.length > 0) {
    console.error('\nContrast checks failed:');
    for (const failure of failures) console.error(`- ${failure}`);
    process.exit(1);
  }
  console.log('\nAll secondary/muted text pairs clear 4.5:1.');
}
