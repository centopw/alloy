import { describe, expect, it } from 'vitest';
import { clampIndex, indexAfterRemoval, moveIndex } from './attachmentPreview';

describe('clampIndex', () => {
  it('keeps a position inside a batch', () => {
    expect(clampIndex(4, 3)).toBe(2);
    expect(clampIndex(-1, 3)).toBe(0);
    expect(clampIndex(1, 3)).toBe(1);
  });

  it('falls back to the first file for an empty or broken batch', () => {
    expect(clampIndex(0, 0)).toBe(0);
    expect(clampIndex(NaN, 3)).toBe(0);
    expect(clampIndex(1.7, 4)).toBe(1);
  });
});

describe('moveIndex', () => {
  it('stops at both ends instead of wrapping around', () => {
    expect(moveIndex(0, 3, -1)).toBe(0);
    expect(moveIndex(2, 3, 1)).toBe(2);
    expect(moveIndex(1, 3, 1)).toBe(2);
    expect(moveIndex(1, 3, -1)).toBe(0);
  });

  it('walks a whole album without leaving it', () => {
    let index = 0;
    for (let step = 0; step < 10; step++) index = moveIndex(index, 4, 1);
    expect(index).toBe(3);
  });
});

describe('indexAfterRemoval', () => {
  it('shows the next file in the slot the removed one held', () => {
    expect(indexAfterRemoval(1, 4)).toBe(1);
    expect(indexAfterRemoval(3, 3)).toBe(2);
  });

  it('returns to the first file once nothing is left', () => {
    expect(indexAfterRemoval(2, 0)).toBe(0);
  });
});
