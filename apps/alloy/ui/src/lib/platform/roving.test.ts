import { describe, expect, it } from 'vitest';
import { nextRovingIndex, rovingTabindex } from './roving';

describe('nextRovingIndex', () => {
  const horizontal = { orientation: 'horizontal' as const };

  it('moves a horizontal tablist with Left/Right and wraps', () => {
    expect(nextRovingIndex(0, 3, 'ArrowRight', horizontal)).toBe(1);
    expect(nextRovingIndex(2, 3, 'ArrowRight', horizontal)).toBe(0);
    expect(nextRovingIndex(0, 3, 'ArrowLeft', horizontal)).toBe(2);
  });

  it('ignores arrows that belong to another axis', () => {
    expect(nextRovingIndex(0, 3, 'ArrowDown', horizontal)).toBeNull();
    expect(
      nextRovingIndex(0, 3, 'ArrowRight', { orientation: 'vertical' })
    ).toBeNull();
  });

  it('supports a vertical radiogroup', () => {
    const vertical = { orientation: 'vertical' as const };
    expect(nextRovingIndex(0, 3, 'ArrowDown', vertical)).toBe(1);
    expect(nextRovingIndex(2, 3, 'ArrowDown', vertical)).toBe(0);
    expect(nextRovingIndex(0, 3, 'ArrowUp', vertical)).toBe(2);
  });

  it('honours RTL for horizontal movement', () => {
    expect(nextRovingIndex(0, 3, 'ArrowRight', { orientation: 'horizontal', rtl: true })).toBe(2);
    expect(nextRovingIndex(0, 3, 'ArrowLeft', { orientation: 'horizontal', rtl: true })).toBe(1);
  });

  it('Home and End jump to the ends', () => {
    expect(nextRovingIndex(1, 4, 'Home', horizontal)).toBe(0);
    expect(nextRovingIndex(1, 4, 'End', horizontal)).toBe(3);
  });

  it('clamps without wrapping when loop is off', () => {
    expect(nextRovingIndex(0, 3, 'ArrowLeft', { ...horizontal, loop: false })).toBe(0);
    expect(nextRovingIndex(2, 3, 'ArrowRight', { ...horizontal, loop: false })).toBe(2);
  });

  it('returns null for keys it does not handle and for empty widgets', () => {
    expect(nextRovingIndex(0, 3, 'Enter', horizontal)).toBeNull();
    expect(nextRovingIndex(0, 0, 'ArrowRight', horizontal)).toBeNull();
  });
});

describe('rovingTabindex', () => {
  it('keeps exactly one child in the tab order', () => {
    expect(rovingTabindex(0, 1)).toBe(-1);
    expect(rovingTabindex(1, 1)).toBe(0);
    expect(rovingTabindex(2, 1)).toBe(-1);
  });

  it('keeps the first child tabbable when nothing is active', () => {
    expect(rovingTabindex(0, -1)).toBe(0);
    expect(rovingTabindex(1, -1)).toBe(-1);
  });
});
