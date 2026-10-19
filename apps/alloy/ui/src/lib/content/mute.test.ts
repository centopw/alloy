import { describe, expect, it } from 'vitest';
import { expiredMutes, isMutedUntil, muteEndLabel, sameMutedUntil, withMute } from './mute';

describe('expiredMutes', () => {
  it('returns only rooms whose mute has ended', () => {
    expect(expiredMutes({ '!a': 100, '!b': 200, '!c': 300 }, 200).toSorted()).toEqual(['!a', '!b']);
    expect(expiredMutes({}, 1)).toEqual([]);
  });
});

describe('withMute', () => {
  it('sets and clears one room without touching the others', () => {
    const base = { '!a': 1 };
    expect(withMute(base, '!b', 5)).toEqual({ '!a': 1, '!b': 5 });
    expect(withMute({ '!a': 1, '!b': 5 }, '!a', null)).toEqual({ '!b': 5 });
    expect(base).toEqual({ '!a': 1 });
  });
});

describe('isMutedUntil and sameMutedUntil', () => {
  it('accepts a map of finite numbers only', () => {
    expect(isMutedUntil({ '!a': 5 })).toBe(true);
    expect(isMutedUntil({ '!a': '5' })).toBe(false);
    expect(isMutedUntil({ '!a': Infinity })).toBe(false);
    expect(isMutedUntil([1])).toBe(false);
    expect(isMutedUntil(null)).toBe(false);
  });

  it('compares by content', () => {
    expect(sameMutedUntil({ '!a': 1 }, { '!a': 1 })).toBe(true);
    expect(sameMutedUntil({ '!a': 1 }, { '!a': 2 })).toBe(false);
    expect(sameMutedUntil({ '!a': 1 }, { '!a': 1, '!b': 1 })).toBe(false);
  });
});

describe('muteEndLabel', () => {
  const fmt = () => '3:40 PM';
  it('is null without a running timer', () => {
    expect(muteEndLabel(undefined, 10, fmt)).toBeNull();
    expect(muteEndLabel(5, 10, fmt)).toBeNull();
  });

  it('names the end time, marking another day', () => {
    const now = new Date(2026, 9, 19, 10, 0).getTime();
    expect(muteEndLabel(new Date(2026, 9, 19, 15, 40).getTime(), now, fmt)).toBe('Muted until 3:40 PM');
    expect(muteEndLabel(new Date(2026, 9, 20, 15, 40).getTime(), now, fmt)).toBe('Muted until tomorrow 3:40 PM');
  });
});
