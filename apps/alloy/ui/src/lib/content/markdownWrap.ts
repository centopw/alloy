/** Formatting the composer toolbar and shortcuts can apply to the selected text. */
export type WrapKind = 'bold' | 'italic' | 'strike' | 'code' | 'spoiler' | 'quote' | 'link';

export interface WrapResult {
  text: string;
  /** Selection after the change. */
  start: number;
  end: number;
}

const MARKS: Record<Exclude<WrapKind, 'quote' | 'link'>, string> = {
  bold: '**',
  italic: '_',
  strike: '~~',
  code: '`',
  spoiler: '||',
};

/** Keyboard shortcut (with Ctrl/Cmd) for each kind: B, I, Shift+X, Shift+M, Shift+P, Shift+Q, Shift+L (Ctrl+K opens the room palette). */
export const WRAP_SHORTCUTS: Record<WrapKind, { key: string; shift: boolean }> = {
  bold: { key: 'b', shift: false },
  italic: { key: 'i', shift: false },
  strike: { key: 'x', shift: true },
  code: { key: 'm', shift: true },
  spoiler: { key: 'p', shift: true },
  quote: { key: 'q', shift: true },
  link: { key: 'l', shift: true },
};

/** The kind a Ctrl/Cmd key press stands for, or null. */
export function wrapKindForKey(key: string, shift: boolean): WrapKind | null {
  const k = key.toLowerCase();
  for (const [kind, s] of Object.entries(WRAP_SHORTCUTS) as [WrapKind, { key: string; shift: boolean }][]) {
    if (s.key === k && s.shift === shift) return kind;
  }
  return null;
}

/**
 * Applies `kind` to the selection `start`..`end` of `text`. Wrapping an already wrapped selection removes
 * the marks. With nothing selected, the marks go in with the caret between them. `url` is the link target.
 */
export function wrapSelection(text: string, start: number, end: number, kind: WrapKind, url = ''): WrapResult {
  const from = Math.max(0, Math.min(start, end));
  const to = Math.min(text.length, Math.max(start, end));
  if (kind === 'quote') return quoteLines(text, from, to);
  if (kind === 'link') {
    const label = text.slice(from, to) || 'link';
    const insert = `[${label}](${url})`;
    return { text: text.slice(0, from) + insert + text.slice(to), start: from + 1, end: from + 1 + label.length };
  }
  const mark = MARKS[kind];
  const selected = text.slice(from, to);
  const before = text.slice(0, from);
  const after = text.slice(to);
  // Selection includes the marks: take them off.
  if (selected.length >= mark.length * 2 && selected.startsWith(mark) && selected.endsWith(mark)) {
    const inner = selected.slice(mark.length, selected.length - mark.length);
    return { text: before + inner + after, start: from, end: from + inner.length };
  }
  // Marks sit just outside the selection: take them off.
  if (before.endsWith(mark) && after.startsWith(mark)) {
    return {
      text: before.slice(0, before.length - mark.length) + selected + after.slice(mark.length),
      start: from - mark.length,
      end: to - mark.length,
    };
  }
  return { text: before + mark + selected + mark + after, start: from + mark.length, end: to + mark.length };
}

/** Prefixes every line touched by the selection with "> ", or removes the prefix when all have it. */
function quoteLines(text: string, from: number, to: number): WrapResult {
  const lineStart = text.lastIndexOf('\n', from - 1) + 1;
  const nl = text.indexOf('\n', to);
  const lineEnd = nl === -1 ? text.length : nl;
  const lines = text.slice(lineStart, lineEnd).split('\n');
  const allQuoted = lines.every((l) => l.startsWith('> '));
  const next = lines.map((l) => (allQuoted ? l.slice(2) : `> ${l}`));
  const body = next.join('\n');
  return { text: text.slice(0, lineStart) + body + text.slice(lineEnd), start: lineStart, end: lineStart + body.length };
}
