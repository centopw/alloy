import { describe, expect, it } from 'vitest';
import { getInitials } from './initials';
import { getUserColorIndex, getUserColorVar, getUserColorValue } from './userColor';

describe('userColor', () => {
  it('is deterministic for the same Matrix user ID', () => {
    const id = '@alice:matrix.org';
    const index1 = getUserColorIndex(id);
    const index2 = getUserColorIndex(id);
    expect(index1).toBe(index2);
    expect(index1).toBeGreaterThanOrEqual(1);
    expect(index1).toBeLessThanOrEqual(12);

    expect(getUserColorVar(id)).toBe(`--alloy-color-user-${index1}`);
    expect(getUserColorValue(id)).toBe(`var(--alloy-color-user-${index1})`);
  });

  it('matches stability snapshot of 10 known IDs', () => {
    const knownSnapshots = [
      { id: '@alice:example.org', expectedIndex: 2 },
      { id: '@bob:example.org', expectedIndex: 7 },
      { id: '@charlie:example.org', expectedIndex: 4 },
      { id: '@david:example.org', expectedIndex: 12 },
      { id: '@emma:example.org', expectedIndex: 12 },
      { id: '@frank:example.org', expectedIndex: 10 },
      { id: '@grace:example.org', expectedIndex: 6 },
      { id: '@heidi:example.org', expectedIndex: 1 },
      { id: '@ivan:example.org', expectedIndex: 8 },
      { id: '@judy:example.org', expectedIndex: 6 }
    ];

    for (const { id, expectedIndex } of knownSnapshots) {
      expect(getUserColorIndex(id)).toBe(expectedIndex);
      expect(getUserColorValue(id)).toBe(`var(--alloy-color-user-${expectedIndex})`);
    }
  });

  it('distributes 1000 sample IDs roughly uniformly across the 12 colors', () => {
    const buckets = new Array(12).fill(0);
    const sampleCount = 1000;

    for (let i = 0; i < sampleCount; i++) {
      const id = `@user_${i}:matrix.org`;
      const idx = getUserColorIndex(id);
      buckets[idx - 1]++;
    }

    // Expected count per bucket is ~83.3.
    // Assert every bucket has at least 50 and at most 120 (well within statistical bounds)
    for (let b = 0; b < 12; b++) {
      expect(buckets[b]).toBeGreaterThan(50);
      expect(buckets[b]).toBeLessThan(120);
    }
  });

  it('handles empty or undefined user IDs gracefully', () => {
    expect(getUserColorIndex(undefined)).toBe(1);
    expect(getUserColorIndex('')).toBe(1);
  });
});

describe('initials', () => {
  it('extracts two initials from multi-word display names', () => {
    expect(getInitials('Alice Bob')).toBe('AB');
    expect(getInitials('Alice Charlie Bob')).toBe('AB');
    expect(getInitials('John Doe Smith')).toBe('JS');
  });

  it('extracts two letters from a single word name if available', () => {
    expect(getInitials('Alice')).toBe('AL');
    expect(getInitials('Bob')).toBe('BO');
    expect(getInitials('A')).toBe('A');
  });

  it('strips decorative emoji from display names', () => {
    expect(getInitials('🚀 Alice Bob')).toBe('AB');
    expect(getInitials('Carol ✨')).toBe('CA');
    expect(getInitials('🔥 💯')).toBe('?');
  });

  it('falls back to Matrix ID when display name is empty or only emoji', () => {
    expect(getInitials('🎉', '@bob:matrix.org')).toBe('BO');
    expect(getInitials('', '@alice_smith:example.com')).toBe('AL');
    expect(getInitials(null, '@z:example.com')).toBe('Z');
  });

  it('handles CJK characters properly', () => {
    expect(getInitials('李小龙')).toBe('李小');
    expect(getInitials('山田太郎')).toBe('山田');
    expect(getInitials('김철수')).toBe('김철');
    expect(getInitials('王')).toBe('王');
  });

  it('returns ? for completely empty inputs', () => {
    expect(getInitials('')).toBe('?');
    expect(getInitials(null)).toBe('?');
    expect(getInitials(undefined)).toBe('?');
  });
});
