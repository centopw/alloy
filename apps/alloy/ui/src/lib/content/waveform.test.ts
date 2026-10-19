import { describe, expect, it } from 'vitest';
import { barIndexAt, downsample, fallbackBars } from './waveform.js';

describe('downsample', () => {
  it('returns an empty array when bars <= 0', () => {
    expect(downsample([100, 200], 0)).toEqual([]);
    expect(downsample([100, 200], -5)).toEqual([]);
  });

  it('returns zero-filled array when levels is empty', () => {
    const bars = 8;
    const result = downsample([], bars);
    expect(result).toHaveLength(bars);
    expect(result.every((v) => v === 0)).toBe(true);
  });

  it('produces exactly bars output elements', () => {
    const input = [100, 200, 300, 400, 500];
    expect(downsample(input, 3)).toHaveLength(3);
    expect(downsample(input, 10)).toHaveLength(10);
    expect(downsample(input, 64)).toHaveLength(64);
  });

  it('takes the peak (maximum) in each bucket and normalizes to 0..1', () => {
    // 4 buckets from 8 items: [0, 512], [256, 1024], [128, 64], [512, 256]
    // Peaks: 512, 1024, 128, 512
    // Normalized (/ 1024): 0.5, 1.0, 0.125, 0.5
    const levels = [0, 512, 256, 1024, 128, 64, 512, 256];
    const res = downsample(levels, 4);
    expect(res).toEqual([0.5, 1.0, 0.125, 0.5]);
  });

  it('clamps values below 0 and above 1024', () => {
    const levels = [-200, 2048, 1024, 0];
    const res = downsample(levels, 4);
    expect(res).toEqual([0, 1.0, 1.0, 0]);
  });

  it('handles input shorter than the number of bars', () => {
    const levels = [512, 1024];
    const res = downsample(levels, 4);
    expect(res).toHaveLength(4);
    expect(res.every((v) => v >= 0 && v <= 1)).toBe(true);
  });
});

describe('fallbackBars', () => {
  it('returns empty array when bars <= 0', () => {
    expect(fallbackBars('mxc://example.org/audio1', 0)).toEqual([]);
    expect(fallbackBars('mxc://example.org/audio1', -1)).toEqual([]);
  });

  it('produces exactly bars count', () => {
    const res = fallbackBars('mxc://test/123', 64);
    expect(res).toHaveLength(64);
  });

  it('is deterministic for the same seed and bars', () => {
    const a = fallbackBars('mxc://matrix.org/audio123', 32);
    const b = fallbackBars('mxc://matrix.org/audio123', 32);
    expect(a).toEqual(b);
  });

  it('produces different patterns for different seeds', () => {
    const a = fallbackBars('mxc://matrix.org/audioA', 32);
    const b = fallbackBars('mxc://matrix.org/audioB', 32);
    expect(a).not.toEqual(b);
  });

  it('keeps plausible values between 0.1 and 1.0', () => {
    const res = fallbackBars('mxc://seed/sound', 64);
    for (const v of res) {
      expect(v).toBeGreaterThanOrEqual(0.1);
      expect(v).toBeLessThanOrEqual(1.0);
    }
  });
});

describe('barIndexAt', () => {
  it('returns 0 when fraction is 0', () => {
    expect(barIndexAt(0, 64)).toBe(0);
  });

  it('returns bars - 1 when fraction is 1', () => {
    expect(barIndexAt(1, 64)).toBe(63);
  });

  it('returns proportional index in the middle', () => {
    expect(barIndexAt(0.5, 64)).toBe(32);
    expect(barIndexAt(0.25, 64)).toBe(16);
    expect(barIndexAt(0.75, 64)).toBe(48);
  });

  it('clamps fractions below 0 to 0', () => {
    expect(barIndexAt(-0.5, 64)).toBe(0);
  });

  it('clamps fractions above 1 to bars - 1', () => {
    expect(barIndexAt(1.5, 64)).toBe(63);
  });

  it('returns 0 if bars <= 0', () => {
    expect(barIndexAt(0.5, 0)).toBe(0);
  });
});
