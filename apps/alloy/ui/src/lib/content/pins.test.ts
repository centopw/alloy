import { describe, expect, it } from 'vitest';
import { nextPinIndex, pinPreview } from './pins';

describe('nextPinIndex', () => {
  it('steps to the next older pin and wraps to the newest', () => {
    expect(nextPinIndex(2, 3)).toBe(1);
    expect(nextPinIndex(1, 3)).toBe(0);
    expect(nextPinIndex(0, 3)).toBe(2);
  });

  it('stays put with a single pin or none', () => {
    expect(nextPinIndex(0, 1)).toBe(0);
    expect(nextPinIndex(0, 0)).toBe(0);
  });
});

describe('pinPreview', () => {
  it('collapses whitespace and shortens long text', () => {
    expect(pinPreview('a\n\n  b')).toBe('a b');
    expect(pinPreview('x'.repeat(100), 10)).toBe(`${'x'.repeat(9)}\u2026`);
  });

  it('labels an empty message', () => {
    expect(pinPreview('  ')).toBe('Message');
  });
});
