import { describe, expect, it } from 'vitest';
import { firstPreviewUrl } from './urls.js';

describe('firstPreviewUrl', () => {
  it('returns the first http(s) URL in prose', () => {
    expect(firstPreviewUrl('see https://example.com/a and http://b.test')).toBe(
      'https://example.com/a'
    );
  });

  it('strips sentence punctuation after the URL', () => {
    expect(firstPreviewUrl('Read this: https://example.com/post.')).toBe(
      'https://example.com/post'
    );
  });

  it('ignores URLs inside inline code and fenced blocks', () => {
    expect(firstPreviewUrl('`https://a.test` and\n```\nhttps://b.test\n```')).toBeNull();
    expect(firstPreviewUrl('`https://a.test` then https://c.test')).toBe('https://c.test');
  });

  it('skips matrix.to permalinks', () => {
    expect(firstPreviewUrl('https://matrix.to/#/!room:example.org')).toBeNull();
    expect(
      firstPreviewUrl('https://matrix.to/#/!room:example.org https://example.com/x')
    ).toBe('https://example.com/x');
  });

  it('returns null when no URL is present', () => {
    expect(firstPreviewUrl('just words')).toBeNull();
  });
});
