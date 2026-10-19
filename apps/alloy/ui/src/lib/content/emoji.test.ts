import { describe, expect, it } from 'vitest';
import {
  applySkinTone,
  buildEmojiData,
  groupEmoji,
  searchEmoji,
  searchQuery,
  type EmojiEntry,
  type RawEmoji,
  type RawGroup,
} from './emoji';

const groups: RawGroup[] = [
  { key: 'smileys-emotion', message: 'smileys & emotion', order: 0 },
  { key: 'flags', message: 'flags', order: 1 },
  { key: 'component', message: 'component', order: 2 },
];

const raw: RawEmoji[] = [
  {
    hexcode: '1F600',
    label: 'grinning face',
    unicode: '\u{1F600}',
    group: 0,
    order: 1,
    tags: ['smile', 'happy'],
  },
  {
    hexcode: '1F44D',
    label: 'thumbs up',
    unicode: '\u{1F44D}',
    group: 0,
    order: 2,
    tags: ['+1', 'yes'],
    skins: [
      { hexcode: '1F44D-1F3FB', label: 'thumbs up: light skin tone', unicode: '\u{1F44D}\u{1F3FB}' },
      { hexcode: '1F44D-1F3FF', label: 'thumbs up: dark skin tone', unicode: '\u{1F44D}\u{1F3FF}' },
    ],
  },
  {
    hexcode: '2615',
    label: 'hot beverage',
    unicode: '\u{2615}',
    group: 0,
    order: 3,
    tags: ['Caf\u00e9'],
  },
  {
    hexcode: '1F1FA-1F1F8',
    label: 'flag: United States',
    unicode: '\u{1F1FA}\u{1F1F8}',
    group: 1,
    order: 1,
  },
  // Component group: skin-tone swatch, must never be browsable.
  { hexcode: '1F3FB', label: 'light skin tone', unicode: '\u{1F3FB}', group: 2, order: 4 },
  // Regional indicator without a group: must be dropped.
  { hexcode: '1F1E6', label: 'regional indicator a', unicode: '\u{1F1E6}' },
];

function entry(label: string, order = 0, tags: string[] = []): EmojiEntry {
  return { emoji: '', label, hexcode: '', tags, order, skins: [] };
}

function fixture(label: string): EmojiEntry {
  const found = buildEmojiData(raw, groups).all.find((e) => e.label === label);
  if (!found) throw new Error(`missing fixture: ${label}`);
  return found;
}

describe('groupEmoji', () => {
  it('keeps group order and drops the component and ungrouped emoji', () => {
    const grouped = groupEmoji(raw, groups);
    expect(grouped.map((g) => g.key)).toEqual(['smileys-emotion', 'flags']);
    const labels = grouped.flatMap((g) => g.emojis.map((e) => e.label));
    expect(labels).not.toContain('light skin tone');
    expect(labels).not.toContain('regional indicator a');
  });

  it('sorts each group by emojibase order and capitalises headings', () => {
    const [smileys] = groupEmoji(raw, groups);
    expect(smileys.label).toBe('Smileys & emotion');
    expect(smileys.emojis.map((e) => e.label)).toEqual(['grinning face', 'thumbs up', 'hot beverage']);
  });

  it('keeps skin variants on the base entry and out of the flat list', () => {
    const data = buildEmojiData(raw, groups);
    expect(fixture('thumbs up').skins.map((s) => s.emoji)).toEqual([
      '\u{1F44D}\u{1F3FB}',
      '\u{1F44D}\u{1F3FF}',
    ]);
    expect(data.all.map((e) => e.label)).toEqual([
      'grinning face',
      'thumbs up',
      'hot beverage',
      'flag: United States',
    ]);
  });
});

describe('searchQuery', () => {
  it('turns shortcode syntax into a plain phrase', () => {
    expect(searchQuery(':thumbs_up:')).toBe('thumbs up');
  });
});

describe('searchEmoji', () => {
  const all = buildEmojiData(raw, groups).all;

  it('returns nothing for an empty query', () => {
    expect(searchEmoji(all, '')).toEqual([]);
  });

  it('matches tags as well as labels', () => {
    expect(searchEmoji(all, 'smile')[0]?.label).toBe('grinning face');
  });

  it('ranks exact, then prefix, then word prefix, then contains', () => {
    const ranked = [entry('superface'), entry('face palm'), entry('face'), entry('big face')];
    expect(searchEmoji(ranked, 'face').map((e) => e.label)).toEqual([
      'face',
      'face palm',
      'big face',
      'superface',
    ]);
  });

  it('ignores case and diacritics', () => {
    expect(searchEmoji(all, 'CAF\u00c9')[0]?.label).toBe('hot beverage');
    expect(searchEmoji(all, 'cafe')[0]?.label).toBe('hot beverage');
  });

  it('accepts shortcode style queries', () => {
    expect(searchEmoji(all, ':thumbs_up:')[0]?.label).toBe('thumbs up');
  });

  it('returns nothing when no label or tag matches', () => {
    expect(searchEmoji(all, 'zzz')).toEqual([]);
  });

  it('caps results at the limit', () => {
    const many = Array.from({ length: 10 }, (_, i) => entry(`item ${i}`, i));
    expect(searchEmoji(many, 'item', 3)).toHaveLength(3);
  });
});

describe('applySkinTone', () => {
  it('returns the entry unchanged for the default tone', () => {
    const thumbs = fixture('thumbs up');
    expect(applySkinTone(thumbs, 'none')).toBe(thumbs);
  });

  it('returns the matching skin variant', () => {
    const thumbs = fixture('thumbs up');
    expect(applySkinTone(thumbs, 'light').emoji).toBe('\u{1F44D}\u{1F3FB}');
    expect(applySkinTone(thumbs, 'dark').emoji).toBe('\u{1F44D}\u{1F3FF}');
  });

  it('falls back to the base entry when the tone has no variant', () => {
    const thumbs = fixture('thumbs up');
    expect(applySkinTone(thumbs, 'medium')).toBe(thumbs);
  });

  it('leaves entries without skin variants alone', () => {
    const grinning = fixture('grinning face');
    expect(applySkinTone(grinning, 'light')).toBe(grinning);
  });
});
