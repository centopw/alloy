import { describe, expect, it } from 'vitest';
import { movePinned, orderChanged } from './pinOrder';

describe('movePinned', () => {
  const ids = ['a', 'b', 'c', 'd'];

  it('moves an id down to the target index', () => {
    expect(movePinned(ids, 'a', 2)).toEqual(['b', 'c', 'a', 'd']);
  });

  it('moves an id up to the target index', () => {
    expect(movePinned(ids, 'd', 0)).toEqual(['d', 'a', 'b', 'c']);
  });

  it('clamps the target to the list', () => {
    expect(movePinned(ids, 'b', 99)).toEqual(['a', 'c', 'd', 'b']);
    expect(movePinned(ids, 'c', -5)).toEqual(['c', 'a', 'b', 'd']);
  });

  it('returns the same order for an unknown id or a no-op move', () => {
    expect(movePinned(ids, 'zz', 1)).toEqual(ids);
    expect(movePinned(ids, 'b', 1)).toEqual(ids);
  });

  it('never mutates the input', () => {
    const input = ['x', 'y'];
    movePinned(input, 'y', 0);
    expect(input).toEqual(['x', 'y']);
  });
});

describe('orderChanged', () => {
  it('tells a reorder from a no-op', () => {
    expect(orderChanged(['a', 'b'], ['a', 'b'])).toBe(false);
    expect(orderChanged(['a', 'b'], ['b', 'a'])).toBe(true);
    expect(orderChanged(['a'], ['a', 'b'])).toBe(true);
  });
});
