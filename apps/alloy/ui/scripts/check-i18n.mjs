#!/usr/bin/env node
/**
 * Chrome-copy gate for the Svelte UI (decision D15 / issue #34).
 *
 * Checks, all of which must pass for `pnpm check` to be green:
 *
 * 1. **Bundle parity** — `en.ftl` is the source of truth and `vi.ftl` must carry exactly the same
 *    message ids (no missing translation, no stale key). Message content, names and user data are
 *    never localized, so they are not in the bundles at all.
 * 2. **Coverage** — every literal `t('…')` call references an id that exists in `en.ftl`. A typo in
 *    a message id would otherwise silently render the fallback bundle's raw id at runtime.
 * 3. **No hardcoded copy** — user-visible text must come from the bundles, not from a literal:
 *    - plain template text and user-facing attributes (`title`, `placeholder`, `aria-label`, …) that
 *      mix literal text with expressions (`aria-label="Download {name}"`);
 *    - string literals inside template expressions of user-facing attributes that carry prose
 *      (`aria-label={open ? 'Hide emoji picker' : 'Show emoji picker'}`), when they are not the
 *      arguments of `t()`; likewise for text-node expressions (`{`${count} member(s)`}`);
 *    - object literals rendered as UI copy in `<script>` blocks, in template expressions (inline
 *      `use:contextmenu` / menu builders) and in `.ts` modules: the keys in `SCRIPT_COPY_KEYS`
 *      (`label`, `description`, `detail`, `action`, `hint`, …). Their value may be a ternary,
 *      template literal or concatenation, so every branch is inspected, not just a bare literal.
 *
 * Scope: `src/**\/*.svelte` plus `.ts` modules under `src/lib` (`*.test.ts` and the explicit
 * `MODULE_EXCLUDES` entries are protocol/generated/locale plumbing, not chrome copy).
 *
 * Usage: `node scripts/check-i18n.mjs` (from `apps/alloy/ui`).
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
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
  'legend',
  'ariaLabel',
  'ariaDescription',
  'data-tooltip',
]);

/**
 * Object keys whose string values are rendered in the UI (menu labels, hints, placeholders,
 * inline errors, tooltips, notice bodies). Script blocks and data modules are otherwise full of
 * protocol identifiers and diagnostics, so the scan is restricted to these keys rather than all
 * string literals.
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
  'detail',
  'action',
  'confirm',
  'message',
  'disabledReason',
  'why',
]);

/**
 * Values that look like copy keys but are protocol codes. The scan is deliberately narrow: only
 * `action` (IPC command names are compound PascalCase: `GetOwnProfile`, `MarkRoomRead`) and
 * `reason` (failure codes are snake_case or one lowercase word: `launch_failed`, `expired`) need
 * it. UI action copy such as "Retry" or "Download" is a single capitalised word and is still
 * flagged.
 */
const KEY_VALUE_EXCLUDES = {
  action: /^[A-Z][a-z0-9]*(?:[A-Z][a-z0-9]*)+$/,
  reason: /^[a-z][a-z0-9_]*$/,
};

/**
 * Developer-only surfaces (the perf HUD) are tooling, not product chrome.
 */
const EXCLUDED_DIRS = new Set([join(SRC_DIR, 'lib/components/dev')]);

/**
 * Non-UI modules are excluded one directory at a time, with the reason. Do not replace these with
 * broad globs: a new module that renders copy must be scanned.
 */
const MODULE_EXCLUDES = [
  {
    dir: join(SRC_DIR, 'lib/ipc'),
    why: 'IPC payloads: `action:` carries Rust enum variant names (DiscoverHomeserver, …), not copy',
  },
  {
    dir: join(SRC_DIR, 'lib/generated'),
    why: 'generated protocol types/enums; contains no user-facing strings',
  },
  {
    dir: join(SRC_DIR, 'lib/mock'),
    why: 'mock transport fixtures replay protocol payloads and event text, not UI chrome',
  },
  {
    dir: join(SRC_DIR, 'lib/i18n'),
    why: 'locale bundle loading and Fluent plumbing; the bundles themselves are checked separately',
  },
  {
    dir: join(SRC_DIR, 'lib/platform'),
    why: 'OS/browser wrappers, IPC transport and protocol constants (keyboard chords, media types)',
  },
];

/** Literal strings that are not prose and do not need a bundle entry: brand and product names,
 * protocol acronyms, and the standalone key glyphs the shortcut hints render (`<kbd>↵</kbd>`).
 * Matching is exact after trimming; the list stays tiny on purpose - adding a phrase here is a
 * decision, not a convenience. */
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

function lineOf(source, index) {
  let line = 1;
  for (let i = 0; i < index && i < source.length; i++) if (source[i] === '\n') line++;
  return line;
}

/** Returns the index just past the bracket that opens at `i` (quote-aware, nested). */
function skipBalanced(text, i) {
  let depth = 0;
  let quote = null;
  for (; i < text.length; i++) {
    const ch = text[i];
    if (quote) {
      if (ch === '\\') i++;
      else if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      quote = ch;
      continue;
    }
    if (ch === '{' || ch === '(' || ch === '[') depth++;
    else if (ch === '}' || ch === ')' || ch === ']') {
      depth--;
      if (depth === 0) return i + 1;
    }
  }
  return text.length;
}

/**
 * Removes `t('…', { … })` call spans from an expression so its arguments are not mistaken for
 * hardcoded copy. A `t` that is part of a longer identifier or a property access is left alone.
 */
export function stripTCalls(expr) {
  let out = '';
  let i = 0;
  while (i < expr.length) {
    const ch = expr[i];
    const previous = i === 0 ? '' : expr[i - 1];
    if (ch === 't' && !/[\w$.]/.test(previous)) {
      let j = i + 1;
      while (j < expr.length && /\s/.test(expr[j])) j++;
      if (expr[j] === '(') {
        i = skipBalanced(expr, j);
        continue;
      }
    }
    out += ch;
    i++;
  }
  return out;
}

/**
 * String and template literals of an expression, with their span. A template literal reports its
 * static text parts (the pieces between `${…}`), which is what carries copy.
 */
export function stringSegments(expr) {
  const out = [];
  for (let i = 0; i < expr.length; i++) {
    const ch = expr[i];
    if (ch === "'" || ch === '"') {
      let j = i + 1;
      while (j < expr.length) {
        if (expr[j] === '\\') {
          j += 2;
          continue;
        }
        if (expr[j] === ch) break;
        j++;
      }
      out.push({ start: i, end: Math.min(j + 1, expr.length), kind: 'string', value: expr.slice(i + 1, j) });
      i = j;
      continue;
    }
    if (ch === '`') {
      const parts = [];
      let current = '';
      let j = i + 1;
      while (j < expr.length) {
        const next = expr[j];
        if (next === '\\') {
          current += expr[j + 1] ?? '';
          j += 2;
          continue;
        }
        if (next === '`') {
          j++;
          break;
        }
        if (next === '$' && expr[j + 1] === '{') {
          parts.push(current);
          current = '';
          j = skipBalanced(expr, j + 1);
          continue;
        }
        current += next;
        j++;
      }
      parts.push(current);
      out.push({ start: i, end: j, kind: 'template', parts });
      i = j - 1;
      continue;
    }
  }
  return out;
}

/**
 * Splits an expression at top-level operators (`?:`, `+`, `||`, `&&`, `,`) so a ternary branch,
 * template literal or concatenation operand can be inspected on its own. Nested calls/objects and
 * literals are skipped as units, which keeps `format(x, 'short')` from looking like copy.
 */
function topLevelSegments(code) {
  const out = [];
  let start = 0;
  let depth = 0;
  let quote = null;
  for (let i = 0; i < code.length; i++) {
    const ch = code[i];
    if (quote) {
      if (ch === '\\') i++;
      else if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      quote = ch;
      continue;
    }
    if (ch === '(' || ch === '[' || ch === '{') {
      depth++;
      continue;
    }
    if (ch === ')' || ch === ']' || ch === '}') {
      depth = Math.max(0, depth - 1);
      continue;
    }
    if (depth > 0) continue;
    let opLength = 0;
    let opStart = i;
    if (ch === '?') {
      if (code[i + 1] === '.' || code[i + 1] === '?') {
        i++;
        continue;
      }
      opLength = 1;
    } else if (ch === ':') {
      opLength = 1;
    } else if (ch === '+') {
      opLength = 1;
    } else if ((ch === '|' && code[i + 1] === '|') || (ch === '&' && code[i + 1] === '&')) {
      opLength = 2;
    } else if (ch === ',') {
      opLength = 1;
    }
    if (opLength > 0) {
      out.push(code.slice(start, opStart));
      i = opStart + opLength - 1;
      start = opStart + opLength;
    }
  }
  out.push(code.slice(start));
  return out;
}

/** Prose carried by a segment that consists solely of one string/template literal. */
function loneLiteralProse(segment) {
  let text = segment.trim();
  for (;;) {
    if (!text.startsWith('(') || !text.endsWith(')') || skipBalanced(text, 0) !== text.length) break;
    text = text.slice(1, -1).trim();
  }
  if (!text) return [];
  const literals = stringSegments(text);
  if (literals.length !== 1) return [];
  const [literal] = literals;
  if (literal.start !== 0 || literal.end !== text.length) return [];
  const parts = literal.kind === 'template' ? literal.parts : [literal.value];
  return parts.map((part) => part.trim()).filter((part) => part && !isAllowedLiteral(part));
}

/** Every prose literal that is displayed directly by the expression. */
export function expressionProseLiterals(expr) {
  const out = [];
  for (const segment of topLevelSegments(stripTCalls(expr))) {
    for (const prose of loneLiteralProse(segment)) out.push(prose);
  }
  return out;
}

/** Removes `{…}` expressions from a string, keeping the literal text around them. */
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

/** Literal spans of a user-facing attribute value; `[]` when there is no literal prose. */
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

/** Top-level `{…}` expression spans of a template text segment (including blocks/consts). */
function textExpressions(segment) {
  const out = [];
  let depth = 0;
  let start = -1;
  let quote = null;
  for (let i = 0; i < segment.length; i++) {
    const ch = segment[i];
    if (depth === 0) {
      if (ch === '{') {
        depth = 1;
        start = i;
      }
      continue;
    }
    if (quote) {
      if (ch === '\\') i++;
      else if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      quote = ch;
      continue;
    }
    if (ch === '{') depth++;
    else if (ch === '}') {
      depth--;
      if (depth === 0) out.push({ start, text: segment.slice(start + 1, i) });
    }
  }
  return out;
}

/** `name={expression}` values of an opening tag, with their offset inside the tag source. */
export function attributeExpressions(tag) {
  const out = [];
  const re = /([A-Za-z@:*_\-[\]().!#]+)\s*=\s*\{/g;
  let match;
  while ((match = re.exec(tag)) !== null) {
    const open = match.index + match[0].length - 1;
    const end = skipBalanced(tag, open);
    out.push({ name: match[1], start: match.index, text: tag.slice(open + 1, end - 1) });
    re.lastIndex = end;
  }
  return out;
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

/**
 * Value span of `key: <value>` starting after the colon: stops at a top-level comma/semicolon or at
 * the closing bracket of the enclosing object. Nested calls/objects/literals are consumed whole.
 */
function valueSpan(source, start) {
  let i = start;
  let depth = 0;
  let quote = null;
  for (; i < source.length; i++) {
    const ch = source[i];
    if (quote) {
      if (ch === '\\') i++;
      else if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      quote = ch;
      continue;
    }
    if (ch === '(' || ch === '[' || ch === '{') depth++;
    else if (ch === ')' || ch === ']' || ch === '}') {
      if (depth === 0) break;
      depth--;
    } else if (depth === 0 && (ch === ',' || ch === ';')) break;
  }
  return source.slice(start, i);
}

/**
 * Finds user copy in copy-key object literals: `label: 'Reply'`, `detail: cond ? 'A' : 'B'`,
 * `message: (name) => `…``. Used for component script blocks and `.ts` modules alike.
 */
export function copyKeyProblems(source, kindPrefix = 'script') {
  const problems = [];
  const re = /\b([A-Za-z][A-Za-z0-9_]*)\s*:\s*/g;
  let match;
  while ((match = re.exec(source)) !== null) {
    const key = match[1];
    if (!SCRIPT_COPY_KEYS.has(key)) continue;
    let value = valueSpan(source, match.index + match[0].length);
    // A copy key may hold a builder function (`message: (name) => `${name} will go``): inspect the
    // returned expression, not the parameter list.
    const arrow = /^\s*(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>\s*/.exec(value);
    if (arrow) value = value.slice(arrow[0].length);
    const ignored = KEY_VALUE_EXCLUDES[key];
    for (const text of expressionProseLiterals(value)) {
      if (ignored?.test(text)) continue;
      problems.push({
        line: lineOf(source, match.index),
        text: text.replace(/\s+/g, ' ').slice(0, 80),
        kind: `${kindPrefix} ${key}`,
      });
    }
  }
  return problems;
}

/** Copy literals in the `<script>` blocks of a component. */
export function scriptCopy(source) {
  const problems = [];
  const blocks = [...source.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)];
  for (const block of blocks) {
    const body = block[1];
    const base = block.index + block[0].indexOf('>') + 1;
    const lineOffset = lineOf(source, base) - 1;
    for (const problem of copyKeyProblems(body)) {
      problems.push({ ...problem, line: problem.line + lineOffset });
    }
  }
  return problems;
}

/** Copy literals in a `.ts` module: same copy-key object-literal pattern as script blocks. */
export function moduleCopy(source) {
  return copyKeyProblems(source, 'module');
}

/** True when a module path is deliberately skipped by the `.ts` scan (see MODULE_EXCLUDES). */
export function moduleExcluded(file) {
  if (file.endsWith('.test.ts')) return 'test file: fixtures assert behaviour, they are not chrome';
  const entry = MODULE_EXCLUDES.find(({ dir }) => file === dir || file.startsWith(dir + sep));
  return entry ? entry.why : null;
}

/** Literal prose in the template: text nodes, user-facing attributes and their expressions. */
export function hardcodedCopy(source) {
  const template = templateOnly(source);
  const problems = [];
  const { textSegments, tags } = scanTemplate(template);

  const pushText = (index, text, kind) => {
    problems.push({
      line: lineOf(template, index),
      text: text.replace(/\s+/g, ' ').slice(0, 80),
      kind,
    });
  };

  for (const segment of textSegments) {
    const literal = stripExpressions(segment.source).trim();
    if (literal && !isAllowedLiteral(literal)) pushText(segment.start, literal, 'text');
    for (const expression of textExpressions(segment.source)) {
      for (const text of expressionProseLiterals(expression.text)) {
        pushText(segment.start + expression.start, text, 'text interpolation');
      }
      for (const copy of copyKeyProblems(expression.text, 'template')) {
        problems.push({
          line: lineOf(template, segment.start + expression.start) + copy.line - 1,
          text: copy.text,
          kind: copy.kind,
        });
      }
    }
  }

  for (const tag of tags) {
    if (tag.source.startsWith('</') || tag.source.startsWith('<!--')) continue;
    for (const { name, literal } of literalAttributeValues(tag.source)) {
      if (isAllowedLiteral(literal)) continue;
      pushText(tag.start, literal, `attribute ${name}`);
    }
    for (const expression of attributeExpressions(tag.source)) {
      if (USER_FACING_ATTRIBUTES.has(expression.name)) {
        for (const text of expressionProseLiterals(expression.text)) {
          pushText(tag.start + expression.start, text, `attribute ${expression.name}`);
        }
      }
      for (const copy of copyKeyProblems(expression.text, 'template')) {
        problems.push({
          line: lineOf(template, tag.start + expression.start) + copy.line - 1,
          text: copy.text,
          kind: copy.kind,
        });
      }
    }
  }
  return problems;
}

function walk(dir, out = [], extension) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (!EXCLUDED_DIRS.has(full)) walk(full, out, extension);
    } else if (entry.endsWith(extension)) out.push(full);
  }
  return out;
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

  const componentFiles = walk(SRC_DIR, [], '.svelte').sort();
  const moduleFiles = walk(SRC_DIR, [], '.ts')
    .filter((file) => moduleExcluded(file) === null)
    .sort();
  const files = [...componentFiles, ...moduleFiles];
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

    const found = file.endsWith('.svelte')
      ? [...hardcodedCopy(source), ...scriptCopy(source)]
      : moduleCopy(source);
    for (const problem of found) {
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
    `i18n ok: ${enIds.size} messages, en/vi in sync, ${usedIds.size} used ids all defined, no hardcoded chrome copy in ${componentFiles.length} components and ${moduleFiles.length} modules.`
  );
  if (unused.length > 0) {
    console.log(`Note: ${unused.length} en.ftl ids are not referenced by a literal t() call.`);
  }
}

const invokedDirectly =
  process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) main();
