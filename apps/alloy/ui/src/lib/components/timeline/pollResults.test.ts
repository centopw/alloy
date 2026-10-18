import { describe, expect, it } from 'vitest';
import { pollPercent } from './pollResults';

describe('pollPercent', () => {
  it('returns zero when nothing is counted', () => {
    expect(pollPercent(0, 0)).toBe(0);
    expect(pollPercent(3, 0)).toBe(0);
  });

  it('rounds the share of the total to a whole percentage', () => {
    expect(pollPercent(1, 3)).toBe(33);
    expect(pollPercent(2, 3)).toBe(67);
    expect(pollPercent(4, 4)).toBe(100);
  });

  it('never exceeds 100 or drops below 0', () => {
    expect(pollPercent(-1, 5)).toBe(0);
    expect(pollPercent(9, 5)).toBe(100);
  });
});
