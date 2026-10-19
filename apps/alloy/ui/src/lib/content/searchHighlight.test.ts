import { describe, expect, it } from 'vitest';
import { highlightRuns, matchesQuery, normalizeForSearch } from './searchHighlight';

describe('normalizeForSearch', () => {
  it('lowercases and strips accents', () => {
    expect(normalizeForSearch('Café Crème')).toBe('cafe creme');
  });
});

describe('matchesQuery', () => {
  it('ignores case and accents', () => {
    expect(matchesQuery('Hi from Bob', 'from bob')).toBe(true);
    expect(matchesQuery('un café', 'CAFE')).toBe(true);
  });

  it('never matches a blank query', () => {
    expect(matchesQuery('anything', '   ')).toBe(false);
  });

  it('reports a miss', () => {
    expect(matchesQuery('hello', 'bye')).toBe(false);
  });
});

describe('highlightRuns', () => {
  it('marks every occurrence and keeps the original text exact', () => {
    const runs = highlightRuns('Bob said hi, bob!', 'bob');
    expect(runs).toEqual([
      { text: 'Bob', match: true },
      { text: ' said hi, ', match: false },
      { text: 'bob', match: true },
      { text: '!', match: false },
    ]);
    expect(runs.map((r) => r.text).join('')).toBe('Bob said hi, bob!');
  });

  it('maps matches back across accented characters', () => {
    const text = 'Un CAFÉ noir';
    const runs = highlightRuns(text, 'cafe');
    expect(runs).toEqual([
      { text: 'Un ', match: false },
      { text: 'CAFÉ', match: true },
      { text: ' noir', match: false },
    ]);
  });

  it('returns the whole text as one plain run when nothing matches', () => {
    expect(highlightRuns('hello', 'zzz')).toEqual([{ text: 'hello', match: false }]);
  });

  it('returns one plain run for a blank query', () => {
    expect(highlightRuns('hello', '  ')).toEqual([{ text: 'hello', match: false }]);
  });

  it('keeps markup-looking text as plain text', () => {
    const runs = highlightRuns('<b>x</b>', 'x');
    expect(runs.map((r) => r.text).join('')).toBe('<b>x</b>');
    expect(runs.find((r) => r.match)?.text).toBe('x');
  });
});
