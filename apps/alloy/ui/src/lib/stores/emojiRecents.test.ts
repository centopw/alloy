import { describe, expect, it } from 'vitest';
import {
  EMOJI_TONE_KEY,
  MAX_RECENT_EMOJI,
  RECENT_EMOJI_KEY,
  loadRecents,
  loadSkinTone,
  saveRecents,
  saveSkinTone,
  touchRecent,
  type RecentEmoji,
} from './emojiRecents';

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => {
      data.set(key, value);
    },
  };
}

function brokenStorage() {
  return {
    getItem: (): string | null => {
      throw new Error('storage denied');
    },
    setItem: (): void => {
      throw new Error('storage denied');
    },
  };
}

function recents(n: number): RecentEmoji[] {
  return Array.from({ length: n }, (_, i) => ({ emoji: `e${i}`, count: 1 }));
}

describe('loadRecents', () => {
  it('is empty without storage', () => {
    expect(loadRecents(null)).toEqual([]);
  });

  it('round-trips through saveRecents', () => {
    const storage = memoryStorage();
    const list = [
      { emoji: '\u{1F600}', count: 2 },
      { emoji: '\u{2615}', count: 1 },
    ];
    saveRecents(storage, list);
    expect(storage.data.has(RECENT_EMOJI_KEY)).toBe(true);
    expect(loadRecents(storage)).toEqual(list);
  });

  it('treats corrupt JSON as empty', () => {
    expect(loadRecents(memoryStorage({ [RECENT_EMOJI_KEY]: '{not json' }))).toEqual([]);
  });

  it('treats non-array JSON as empty', () => {
    expect(loadRecents(memoryStorage({ [RECENT_EMOJI_KEY]: '{"emoji":"x"}' }))).toEqual([]);
  });

  it('drops malformed entries and keeps the first copy of a duplicate', () => {
    const stored = JSON.stringify([
      { emoji: 'a', count: 1 },
      { emoji: 'a', count: 3 },
      { emoji: '', count: 1 },
      { emoji: 'b', count: 0 },
      { emoji: 'c', count: 1.5 },
      { count: 2 },
      null,
      'x',
      { emoji: 'd', count: 4 },
    ]);
    expect(loadRecents(memoryStorage({ [RECENT_EMOJI_KEY]: stored }))).toEqual([
      { emoji: 'a', count: 1 },
      { emoji: 'd', count: 4 },
    ]);
  });

  it('caps the loaded list at MAX_RECENT_EMOJI', () => {
    const stored = JSON.stringify(recents(40));
    const loaded = loadRecents(memoryStorage({ [RECENT_EMOJI_KEY]: stored }));
    expect(loaded).toHaveLength(MAX_RECENT_EMOJI);
    expect(loaded[0]).toEqual({ emoji: 'e0', count: 1 });
  });

  it('treats throwing storage as empty', () => {
    expect(loadRecents(brokenStorage())).toEqual([]);
  });
});

describe('touchRecent', () => {
  it('puts a new emoji first with count 1', () => {
    expect(touchRecent([{ emoji: 'a', count: 2 }], 'b')).toEqual([
      { emoji: 'b', count: 1 },
      { emoji: 'a', count: 2 },
    ]);
  });

  it('moves an existing emoji to the front and bumps its count', () => {
    expect(
      touchRecent(
        [
          { emoji: 'a', count: 1 },
          { emoji: 'b', count: 5 },
          { emoji: 'c', count: 2 },
        ],
        'b',
      ),
    ).toEqual([
      { emoji: 'b', count: 6 },
      { emoji: 'a', count: 1 },
      { emoji: 'c', count: 2 },
    ]);
  });

  it('caps at MAX_RECENT_EMOJI, dropping the least recent', () => {
    const full = recents(MAX_RECENT_EMOJI);
    const next = touchRecent(full, 'new');
    expect(next).toHaveLength(MAX_RECENT_EMOJI);
    expect(next[0]).toEqual({ emoji: 'new', count: 1 });
    expect(next.some((r) => r.emoji === `e${MAX_RECENT_EMOJI - 1}`)).toBe(false);
  });

  it('does not mutate its input', () => {
    const list: RecentEmoji[] = [{ emoji: 'a', count: 1 }];
    touchRecent(list, 'b');
    expect(list).toEqual([{ emoji: 'a', count: 1 }]);
  });
});

describe('saveRecents', () => {
  it('does nothing without storage', () => {
    expect(() => saveRecents(null, recents(1))).not.toThrow();
  });

  it('swallows storage errors', () => {
    expect(() => saveRecents(brokenStorage(), recents(1))).not.toThrow();
  });
});

describe('skin tone persistence', () => {
  it('defaults to none without storage', () => {
    expect(loadSkinTone(null)).toBe('none');
  });

  it('loads a stored valid tone', () => {
    expect(loadSkinTone(memoryStorage({ [EMOJI_TONE_KEY]: 'dark' }))).toBe('dark');
  });

  it('rejects unknown stored values', () => {
    expect(loadSkinTone(memoryStorage({ [EMOJI_TONE_KEY]: 'purple' }))).toBe('none');
  });

  it('saves and loads the same tone', () => {
    const storage = memoryStorage();
    saveSkinTone(storage, 'medium-light');
    expect(loadSkinTone(storage)).toBe('medium-light');
  });

  it('treats throwing storage as the default tone and never throws on save', () => {
    expect(loadSkinTone(brokenStorage())).toBe('none');
    expect(() => saveSkinTone(brokenStorage(), 'dark')).not.toThrow();
  });
});
