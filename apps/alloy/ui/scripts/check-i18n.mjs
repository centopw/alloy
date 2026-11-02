#!/usr/bin/env node
/**
 * Chrome-copy gate for the Svelte UI (decision D15 / issue #34).
 *
 * Three checks, all of which must pass for `pnpm check` to be green:
 *
 * 1. **Bundle parity** — `en.ftl` is the source of truth and `vi.ftl` must carry exactly the same
 *    message ids (no missing translation, no stale key). Message content, names and user data are
 *    never localized, so they are not in the bundles at all.
 * 2. **Coverage** — every literal `t('…')` call references an id that exists in `en.ftl`. A typo in
 *    a message id would otherwise silently render the fallback bundle's raw id at runtime.
 * 3. **No hardcoded copy** — user-visible text in a component template, or in a user-visible
 *    attribute (`title`, `placeholder`, `aria-label`, `aria-description`, `aria-valuetext`,
 *    `aria-roledescription`, `data-tooltip`, `label`), must come from the bundles, not from a
 *    literal. A string produced by `{t('…')}` or any other Svelte expression is fine; a literal
 *    mixed with expressions (`aria-label="Download {name}"`) is not, because `Download` would stay
 *    English in every locale.
 *
 * Scope: `src/**\/*.svelte`. Script blocks are not scanned for copy (they are mostly protocol
 * constants and diagnostics); that limitation is documented in docs/decisions/0015 and tracked as
 * follow-up on #34.
 *
 * Usage: `node scripts/check-i18n.mjs` (from `apps/alloy/ui`).
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const UI_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC_DIR = join(UI_DIR, 'src');
const EN = join(UI_DIR, 'src/lib/i18n/en.ftl');
const VI = join(UI_DIR, 'src/lib/i18n/vi.ftl');

/** Attributes whose value reaches the user (visually or through assistive tech). */
const USER_FACING_ATTRIBUTES = new Set([
  'title',
  'placeholder',
  'aria-label',
  'aria-description',
  'aria-valuetext',
  'aria-roledescription',
  'alt',
  'label',
  'data-tooltip',
]);

/**
 * Object keys whose string values are rendered in the UI (menu labels, hints, placeholders,
 * inline errors, tooltips). Script blocks are otherwise full of protocol identifiers and
 * diagnostics, so the scan is restricted to these keys rather than all string literals.
 */
const SCRIPT_COPY_KEYS = new Set([
  'label',
  'description',
  'ariaLabel',
  'ariaDescription',
  'hint',
  'reason',
  'placeholder',
  'title',
  'tooltip',
  'toast',
  'errorText',
  'errorMessage',
  'emptyText',
]);

/** Finds `label: 'literal'`-style user copy inside the component's `<script>` blocks. */
export function scriptCopy(source) {
  const problems = [];
  const blocks = [...source.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)];
  for (const block of blocks) {
    const body = block[1];
    const base = block.index + block[0].indexOf('>') + 1;
    const re = /\b([A-Za-z]+)\s*:\s*(['"])((?:\\.|(?!\2)[^\\])*)\2/g;
    let match;
    while ((match = re.exec(body)) !== null) {
      const key = match[1];
      if (!SCRIPT_COPY_KEYS.has(key)) continue;
      const literal = match[3].trim();
      if (!literal || isAllowedLiteral(literal)) continue;
      problems.push({
        line: lineOf(source, base + match.index),
        text: literal.replace(/\s+/g, ' ').slice(0, 80),
        kind: `script ${key}`,
      });
    }
  }
  return problems;
}

/**
 * Literal strings that are not prose and do not need a bundle entry: brand and product names,
 * protocol acronyms, and the standalone key glyphs the shortcut hints render (`<kbd>↵</kbd>`).
 * Matching is exact after trimming; the list stays tiny on purpose - adding a phrase here is a
 * decision, not a convenience.
 */
const ALLOWED_LITERALS = new Set([
  'Alloy',
  'Matrix',
  'Element',
  'Alloy Matrix',
  'GitHub',
  'URL',
  'ID',
  'OK',
  'TLS',
  'HTTP',
  'HTTPS',
  'E2EE',
  'AES',
  'SSO',
  'OIDC',
  '↑',
  '↓',
  '↵',
  '←',
  '→',
  'esc',
  'ESC',
]);

/** Acronyms and protocol words (2-6 upper-case letters, optionally with digits) stay as-is. */
const ACRONYM = /^[A-Z][A-Z0-9]{1,5}$/;
/** A run of emoji/pictographs or pure punctuation is not prose. */
const HAS_LETTER = /[A-Za-zÀ-ỹ]/;

/** Message ids from a Fluent file (indented attribute lines are not ids). */
export function messageIds(source) {
  const ids = new Set();
  const re = /^([A-Za-z][A-Za-z0-9_-]*)\s*=/gm;
  let match;
  while ((match = re.exec(source)) !== null) ids.add(match[1]);
  return ids;
}

/** Developer-only surfaces (the perf HUD) are tooling, not product chrome. */
const EXCLUDED_DIRS = new Set([join(SRC_DIR, 'lib/components/dev')]);

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (!EXCLUDED_DIRS.has(full)) walk(full, out);
    } else if (entry.endsWith('.svelte')) out.push(full);
  }
  return out;
}

/**
 * Removes `{…}` expressions from a string, keeping the literal text around them. Handles nested
 * braces and quoted strings so an expression like `{t('x', { count: 2 })}` does not confuse the
 * scanner. Returns the literal remainder.
 */
export function stripExpressions(text) {
  let out = '';
  let depth = 0;
  let quote = null;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (depth === 0) {
      if (ch === '{') {
        depth = 1;
        continue;
      }
      out += ch;
      continue;
    }
    if (quote) {
      if (ch === '\\') i++;
      else if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') quote = ch;
    else if (ch === '{') depth++;
    else if (ch === '}') depth--;
  }
  return out;
}

/**
 * Splits a Svelte template into text segments and opening tags. The scanner is quote- and
 * brace-aware because Svelte expressions can contain `>` and attribute values can contain `{…}`.
 */
export function scanTemplate(source) {
  const textSegments = [];
  const tags = [];
  let i = 0;
  while (i < source.length) {
    const ch = source[i];
    if (ch === '<') {
      let j = i + 1;
      let quote = null;
      let depth = 0;
      for (; j < source.length; j++) {
        const c = source[j];
        if (quote) {
          if (c === '\\') j++;
          else if (c === quote) quote = null;
          continue;
        }
        if (c === '"' || c === "'") {
          quote = c;
          continue;
        }
        if (c === '{') {
          depth++;
          continue;
        }
        if (c === '}') {
          depth = Math.max(0, depth - 1);
          continue;
        }
        if (c === '>' && depth === 0) break;
      }
      tags.push({ start: i, source: source.slice(i, j + 1) });
      i = j + 1;
      continue;
    }
    let j = i;
    let depth = 0;
    let quote = null;
    for (; j < source.length; j++) {
      const c = source[j];
      if (quote) {
        if (c === '\\') j++;
        else if (c === quote) quote = null;
        continue;
      }
      if (c === '{') depth++;
      else if (c === '}') depth = Math.max(0, depth - 1);
      else if (c === '<' && depth === 0) break;
      if (c === "'" || c === '"' || c === '`') quote = c;
    }
    textSegments.push({ start: i, source: source.slice(i, j) });
    i = j;
  }
  return { textSegments, tags };
}

/** Literal spans of a user-facing attribute value; `null` when there is no literal prose. */
export function literalAttributeValues(tag) {
  const found = [];
  const re = /([A-Za-z@:*_\-[\]().!#]+)\s*=\s*("([^"]*)"|'([^']*)')/g;
  let match;
  while ((match = re.exec(tag)) !== null) {
    const name = match[1];
    if (!USER_FACING_ATTRIBUTES.has(name)) continue;
    const value = match[3] ?? match[4] ?? '';
    const literal = stripExpressions(value).trim();
    if (literal && HAS_LETTER.test(literal)) found.push({ name, literal });
  }
  return found;
}

/** Replaces a removed block with the same number of newlines so line numbers stay true. */
function blankOut(block) {
  return '\n'.repeat((block.match(/\n/g) ?? []).length);
}

/** Removes HTML comments, script blocks and style blocks, preserving line numbering. */
export function templateOnly(source) {
  return source
    .replace(/<script\b[\s\S]*?<\/script>/gi, blankOut)
    .replace(/<style\b[\s\S]*?<\/style>/gi, blankOut)
    .replace(/<!--[\s\S]*?-->/g, blankOut);
}

function lineOf(source, index) {
  let line = 1;
  for (let i = 0; i < index && i < source.length; i++) if (source[i] === '\n') line++;
  return line;
}

export function isAllowedLiteral(text) {
  const trimmed = text.trim();
  if (!trimmed) return true;
  if (!HAS_LETTER.test(trimmed)) return true; // glyphs, punctuation, digits
  if (ALLOWED_LITERALS.has(trimmed)) return true;
  if (ACRONYM.test(trimmed)) return true;
  // A single glyph or letter is never prose: brand marks ("A"), initials, the "x" speed suffix.
  if (trimmed.length === 1) return true;
  // "v0.3.1", "100%", "+12" and similar: no word characters beyond a unit/version.
  if (/^[vV]?\d+(\.\d+)*([%s]|px|ms|MB|GB|KB)?$/.test(trimmed)) return true;
  return false;
}

/** Literal prose in the template: text nodes and user-facing attributes. */
export function hardcodedCopy(source) {
  const template = templateOnly(source);
  const problems = [];
  const { textSegments, tags } = scanTemplate(template);
  for (const segment of textSegments) {
    const literal = stripExpressions(segment.source).trim();
    if (!literal || isAllowedLiteral(literal)) continue;
    // Long whitespace runs are formatting; collapse before reporting.
    const shown = literal.replace(/\s+/g, ' ').slice(0, 80);
    problems.push({ line: lineOf(template, segment.start), text: shown, kind: 'text' });
  }
  for (const tag of tags) {
    if (tag.source.startsWith('</') || tag.source.startsWith('<!--')) continue;
    for (const { name, literal } of literalAttributeValues(tag.source)) {
      if (isAllowedLiteral(literal)) continue;
      problems.push({
        line: lineOf(template, tag.start),
        text: literal.replace(/\s+/g, ' ').slice(0, 80),
        kind: `attribute ${name}`,
      });
    }
  }
  return problems;
}

function main() {
  const enSource = readFileSync(EN, 'utf8');
  const viSource = readFileSync(VI, 'utf8');
  const enIds = messageIds(enSource);
  const viIds = messageIds(viSource);
  const problems = [];

  const missingVi = [...enIds].filter((id) => !viIds.has(id)).sort();
  const staleVi = [...viIds].filter((id) => !enIds.has(id)).sort();
  for (const id of missingVi) problems.push(`vi.ftl: missing translation for "${id}"`);
  for (const id of staleVi) problems.push(`vi.ftl: stale key "${id}" is not in en.ftl`);

  const files = walk(SRC_DIR).sort();
  const usedIds = new Map();
  for (const file of files) {
    const source = readFileSync(file, 'utf8');
    const rel = relative(UI_DIR, file);

    // t('literal'), t("literal") and ${t('literal')} cover every call site. Dynamic ids (a
    // template literal or a variable) cannot be checked statically and are skipped.
    const callRe = /\bt\(\s*(['"])([A-Za-z0-9_.-]+)\1/g;
    let match;
    while ((match = callRe.exec(source)) !== null) {
      const id = match[2];
      if (!enIds.has(id)) {
        problems.push(`${rel}: t('${id}') has no message in en.ftl`);
      } else {
        usedIds.set(id, (usedIds.get(id) ?? 0) + 1);
      }
    }

    if (!file.endsWith('.svelte')) continue;
    for (const problem of hardcodedCopy(source)) {
      problems.push(`${rel}:${problem.line}: hardcoded ${problem.kind}: "${problem.text}"`);
    }
    for (const problem of scriptCopy(source)) {
      problems.push(`${rel}:${problem.line}: hardcoded ${problem.kind}: "${problem.text}"`);
    }
  }

  const unused = [...enIds].filter((id) => !usedIds.has(id)).sort();

  if (problems.length > 0) {
    console.error(`i18n check failed with ${problems.length} problem(s):\n`);
    for (const problem of problems) console.error(`  ${problem}`);
    if (unused.length > 0) {
      console.error(`\nUnused en.ftl message ids (not itself a failure, listed for the sweep):`);
      for (const id of unused) console.error(`  ${id}`);
    }
    process.exitCode = 1;
    return;
  }

  console.log(
    `i18n ok: ${enIds.size} messages, en/vi in sync, ${usedIds.size} used ids all defined, no hardcoded chrome copy in ${files.length} components.`
  );
  if (unused.length > 0) {
    console.log(`Note: ${unused.length} en.ftl ids are not referenced by a literal t() call.`);
  }
}

const invokedDirectly =
  process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) main();
