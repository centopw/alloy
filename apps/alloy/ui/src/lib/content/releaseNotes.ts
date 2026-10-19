/**
 * Release notes are parsed into typed blocks and rendered as Svelte text, never as HTML. Anything the
 * parser does not recognise stays plain text, so a manifest cannot inject markup.
 */

export type ReleaseNoteInline =
  | { kind: 'text'; text: string }
  | { kind: 'strong'; text: string }
  | { kind: 'code'; text: string }
  | { kind: 'link'; text: string; href: string };

export type ReleaseNoteBlock =
  | { kind: 'heading'; level: 2 | 3; inlines: ReleaseNoteInline[] }
  | { kind: 'paragraph'; inlines: ReleaseNoteInline[] }
  | { kind: 'list'; items: ReleaseNoteInline[][] }
  | { kind: 'code'; text: string }
  | { kind: 'rule' };

const INLINE_PATTERN = /\*\*([^*\n]+)\*\*|`([^`\n]+)`|\[([^\]\n]+)\]\(([^)\s]+)\)/g;
const HTTP_URL = /^https?:\/\/\S+$/i;
const FENCE = /^```/;
const RULE = /^(?:-{3,}|\*{3,}|_{3,})$/;
const HEADING = /^(#{1,3})\s+(.*)$/;
const LIST_ITEM = /^[-*+]\s+(.*)$/;

/** Split one line of text into plain, bold, code, and link runs. Only http(s) links become links. */
export function parseInline(text: string): ReleaseNoteInline[] {
  const runs: ReleaseNoteInline[] = [];
  /** Adjacent plain text (an unsafe link left as text, a stray marker) stays one run. */
  const pushText = (chunk: string): void => {
    const prev = runs[runs.length - 1];
    if (prev?.kind === 'text') prev.text += chunk;
    else runs.push({ kind: 'text', text: chunk });
  };
  let last = 0;
  for (const match of text.matchAll(INLINE_PATTERN)) {
    const start = match.index ?? 0;
    if (start > last) pushText(text.slice(last, start));
    const [whole, bold, code, label, href] = match;
    if (bold !== undefined) runs.push({ kind: 'strong', text: bold });
    else if (code !== undefined) runs.push({ kind: 'code', text: code });
    else if (href !== undefined && HTTP_URL.test(href)) runs.push({ kind: 'link', text: label, href });
    else pushText(whole);
    last = start + whole.length;
  }
  if (last < text.length) pushText(text.slice(last));
  return runs;
}

/** Parse the release notes markdown from the update manifest into renderable blocks. */
export function parseReleaseNotes(markdown: string): ReleaseNoteBlock[] {
  const blocks: ReleaseNoteBlock[] = [];
  let paragraph: string[] = [];
  let list: ReleaseNoteInline[][] | null = null;
  let code: string[] | null = null;

  const endParagraph = (): void => {
    if (paragraph.length === 0) return;
    blocks.push({ kind: 'paragraph', inlines: parseInline(paragraph.join(' ')) });
    paragraph = [];
  };
  const endList = (): void => {
    if (list === null) return;
    blocks.push({ kind: 'list', items: list });
    list = null;
  };

  for (const raw of markdown.replace(/\r\n?/g, '\n').split('\n')) {
    if (code !== null) {
      if (FENCE.test(raw.trim())) {
        blocks.push({ kind: 'code', text: code.join('\n') });
        code = null;
      } else {
        code.push(raw);
      }
      continue;
    }

    const line = raw.trim();
    if (FENCE.test(line)) {
      endParagraph();
      endList();
      code = [];
      continue;
    }
    if (line === '') {
      endParagraph();
      endList();
      continue;
    }
    if (RULE.test(line)) {
      endParagraph();
      endList();
      blocks.push({ kind: 'rule' });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      endParagraph();
      endList();
      blocks.push({ kind: 'heading', level: heading[1].length === 1 ? 2 : 3, inlines: parseInline(heading[2]) });
      continue;
    }
    const item = LIST_ITEM.exec(line);
    if (item) {
      endParagraph();
      if (list === null) list = [];
      list.push(parseInline(item[1]));
      continue;
    }
    endList();
    paragraph.push(line);
  }

  if (code !== null) blocks.push({ kind: 'code', text: code.join('\n') });
  endParagraph();
  endList();
  return blocks;
}
