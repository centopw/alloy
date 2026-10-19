import { describe, expect, it } from 'vitest';
import { isDoubleTap } from './doubleTap';

const tap = (x: number, y: number, at: number) => ({ x, y, at });

describe('isDoubleTap', () => {
  it('needs an earlier tap', () => {
    expect(isDoubleTap(null, tap(10, 10, 0))).toBe(false);
  });

  it('accepts two close taps in quick succession', () => {
    expect(isDoubleTap(tap(100, 100, 1000), tap(108, 104, 1250))).toBe(true);
  });

  it('rejects taps that are too far apart in time', () => {
    expect(isDoubleTap(tap(100, 100, 1000), tap(100, 100, 1400))).toBe(false);
  });

  it('rejects taps that land far apart', () => {
    expect(isDoubleTap(tap(100, 100, 1000), tap(160, 100, 1100))).toBe(false);
  });
});
