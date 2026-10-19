import { describe, expect, it } from 'vitest';
import { wrapKindForKey, wrapSelection } from './markdownWrap';

describe('wrapSelection', () => {
  it('wraps the selection and keeps it selected', () => {
    expect(wrapSelection('say hello now', 4, 9, 'bold')).toEqual({ text: 'say **hello** now', start: 6, end: 11 });
    expect(wrapSelection('x', 0, 1, 'spoiler')).toEqual({ text: '||x||', start: 2, end: 3 });
  });

  it('puts the caret between the marks when nothing is selected', () => {
    expect(wrapSelection('ab', 1, 1, 'code')).toEqual({ text: 'a``b', start: 2, end: 2 });
  });

  it('unwraps when the marks are inside or just outside the selection', () => {
    expect(wrapSelection('a **b** c', 2, 7, 'bold')).toEqual({ text: 'a b c', start: 2, end: 3 });
    expect(wrapSelection('a **b** c', 4, 5, 'bold')).toEqual({ text: 'a b c', start: 2, end: 3 });
  });

  it('makes a link from the selection, or a placeholder label', () => {
    expect(wrapSelection('see docs', 4, 8, 'link', 'https://x.org')).toEqual({ text: 'see [docs](https://x.org)', start: 5, end: 9 });
    expect(wrapSelection('', 0, 0, 'link', 'https://x.org').text).toBe('[link](https://x.org)');
  });

  it('quotes every touched line and toggles the quote off again', () => {
    const quoted = wrapSelection('one\ntwo\nthree', 2, 5, 'quote');
    expect(quoted.text).toBe('> one\n> two\nthree');
    expect(wrapSelection(quoted.text, quoted.start, quoted.end, 'quote').text).toBe('one\ntwo\nthree');
  });

  it('accepts a backwards selection', () => {
    expect(wrapSelection('abc', 3, 0, 'italic').text).toBe('_abc_');
  });
});

describe('wrapKindForKey', () => {
  it('maps the documented shortcuts', () => {
    expect(wrapKindForKey('b', false)).toBe('bold');
    expect(wrapKindForKey('I', false)).toBe('italic');
    expect(wrapKindForKey('x', true)).toBe('strike');
    expect(wrapKindForKey('m', true)).toBe('code');
    expect(wrapKindForKey('p', true)).toBe('spoiler');
    expect(wrapKindForKey('q', true)).toBe('quote');
    expect(wrapKindForKey('l', true)).toBe('link');
  });

  it('ignores keys without a meaning', () => {
    expect(wrapKindForKey('b', true)).toBeNull();
    expect(wrapKindForKey('z', false)).toBeNull();
  });
});
