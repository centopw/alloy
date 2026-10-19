import { describe, expect, it } from 'vitest';
import { parseInline, parseReleaseNotes } from './releaseNotes';

describe('parseInline', () => {
  it('keeps markup-looking text as plain text', () => {
    expect(parseInline('<script>alert(1)</script>')).toEqual([{ kind: 'text', text: '<script>alert(1)</script>' }]);
  });

  it('turns only http and https links into links', () => {
    expect(parseInline('[notes](https://example.com/a)')).toEqual([
      { kind: 'link', text: 'notes', href: 'https://example.com/a' },
    ]);
    expect(parseInline('[click](javascript:alert(1))')).toEqual([
      { kind: 'text', text: '[click](javascript:alert(1))' },
    ]);
  });

  it('parses bold and code runs around plain text', () => {
    expect(parseInline('Fixed **sync** in `room.rs`.')).toEqual([
      { kind: 'text', text: 'Fixed ' },
      { kind: 'strong', text: 'sync' },
      { kind: 'text', text: ' in ' },
      { kind: 'code', text: 'room.rs' },
      { kind: 'text', text: '.' },
    ]);
  });

  it('leaves unclosed markers as text', () => {
    expect(parseInline('a ** b and `c')).toEqual([{ kind: 'text', text: 'a ** b and `c' }]);
  });

  it('does not treat emphasis inside code as bold', () => {
    expect(parseInline('`**x**`')).toEqual([{ kind: 'code', text: '**x**' }]);
  });
});

describe('parseReleaseNotes', () => {
  it('returns no blocks for empty notes', () => {
    expect(parseReleaseNotes('  \n\n')).toEqual([]);
  });

  it('parses headings, paragraphs, lists, and rules', () => {
    const blocks = parseReleaseNotes(
      ['# Alloy 1.2', 'Faster timeline.', 'Second line.', '', '- First', '* Second', '', '---', '## Fixes'].join('\n'),
    );
    expect(blocks).toEqual([
      { kind: 'heading', level: 2, inlines: [{ kind: 'text', text: 'Alloy 1.2' }] },
      { kind: 'paragraph', inlines: [{ kind: 'text', text: 'Faster timeline. Second line.' }] },
      {
        kind: 'list',
        items: [[{ kind: 'text', text: 'First' }], [{ kind: 'text', text: 'Second' }]],
      },
      { kind: 'rule' },
      { kind: 'heading', level: 3, inlines: [{ kind: 'text', text: 'Fixes' }] },
    ]);
  });

  it('keeps fenced code verbatim, including markup-like lines', () => {
    const blocks = parseReleaseNotes('```\n<img src=x onerror=alert(1)>\n**not bold**\n```');
    expect(blocks).toEqual([{ kind: 'code', text: '<img src=x onerror=alert(1)>\n**not bold**' }]);
  });

  it('renders injected HTML in notes as text without emitting any element nodes', () => {
    const blocks = parseReleaseNotes('<img src=x onerror="alert(1)"> hello\n\n- <b>bold?</b>');
    expect(blocks).toEqual([
      { kind: 'paragraph', inlines: [{ kind: 'text', text: '<img src=x onerror="alert(1)"> hello' }] },
      { kind: 'list', items: [[{ kind: 'text', text: '<b>bold?</b>' }]] },
    ]);
  });

  it('handles CRLF line endings', () => {
    expect(parseReleaseNotes('## A\r\nbody')).toEqual([
      { kind: 'heading', level: 3, inlines: [{ kind: 'text', text: 'A' }] },
      { kind: 'paragraph', inlines: [{ kind: 'text', text: 'body' }] },
    ]);
  });
});
