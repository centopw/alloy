import { describe, expect, it } from 'vitest';
import { isRovingKey, rovingTabStop, rovingTargetIndex } from './rovingFocus';

describe('isRovingKey', () => {
  it('accepts the keys the model moves on', () => {
    for (const key of ['ArrowDown', 'ArrowUp', 'Home', 'End', 'PageDown', 'PageUp']) {
      expect(isRovingKey(key)).toBe(true);
    }
  });

  it('leaves activation keys and typing alone', () => {
    for (const key of ['Enter', ' ', 'Tab', 'Escape', 'a', 'F10']) {
      expect(isRovingKey(key)).toBe(false);
    }
  });
});

describe('rovingTargetIndex', () => {
  it('moves down and up, wrapping at the ends', () => {
    expect(rovingTargetIndex(5, 0, 'ArrowDown')).toBe(1);
    expect(rovingTargetIndex(5, 2, 'ArrowDown')).toBe(3);
    expect(rovingTargetIndex(5, 4, 'ArrowDown')).toBe(0);
    expect(rovingTargetIndex(5, 4, 'ArrowUp')).toBe(3);
    expect(rovingTargetIndex(5, 0, 'ArrowUp')).toBe(4);
  });

  it('jumps to the first and last row with Home and End', () => {
    expect(rovingTargetIndex(40, 21, 'Home')).toBe(0);
    expect(rovingTargetIndex(40, 21, 'End')).toBe(39);
  });

  it('moves a page and clamps at the ends', () => {
    expect(rovingTargetIndex(40, 0, 'PageDown')).toBe(10);
    expect(rovingTargetIndex(40, 35, 'PageDown')).toBe(39);
    expect(rovingTargetIndex(40, 35, 'PageUp')).toBe(25);
    expect(rovingTargetIndex(40, 3, 'PageUp')).toBe(0);
    expect(rovingTargetIndex(40, 0, 'PageDown', { pageSize: 3 })).toBe(3);
  });

  it('starts from the first row when nothing valid is focused', () => {
    expect(rovingTargetIndex(5, -1, 'ArrowDown')).toBe(1);
    expect(rovingTargetIndex(5, 99, 'ArrowDown')).toBe(1);
  });

  it('does nothing for an empty list or a key it does not own', () => {
    expect(rovingTargetIndex(0, 0, 'ArrowDown')).toBeNull();
    expect(rovingTargetIndex(5, 0, 'Enter')).toBeNull();
    expect(rovingTargetIndex(5, 0, 'Tab')).toBeNull();
  });

  it('supports a single-row list without leaving the list', () => {
    expect(rovingTargetIndex(1, 0, 'ArrowDown')).toBe(0);
    expect(rovingTargetIndex(1, 0, 'ArrowUp')).toBe(0);
  });
});

describe('rovingTabStop', () => {
  const ids = ['a', 'b', 'c'];

  it('prefers the row focused last', () => {
    expect(rovingTabStop(ids, 'b', 'a')).toBe('b');
  });

  it('falls back to the open room when the remembered row is gone', () => {
    expect(rovingTabStop(ids, 'gone', 'c')).toBe('c');
  });

  it('falls back to the first row', () => {
    expect(rovingTabStop(ids, null, null)).toBe('a');
    expect(rovingTabStop(ids, 'gone', 'also-gone')).toBe('a');
  });

  it('returns null for an empty list', () => {
    expect(rovingTabStop([], 'b', 'a')).toBeNull();
  });
});
