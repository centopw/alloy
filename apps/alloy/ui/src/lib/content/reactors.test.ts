import { describe, expect, it } from 'vitest';
import { joinNames, reactionTabs, reactorRows } from './reactors';

const reactions = [
  { key: '\u{1F44D}', count: 2, by_me: true, senders: ['@a:hs', '@b:hs'] },
  { key: '\u{2764}\u{FE0F}', count: 1, by_me: false, senders: ['@b:hs'] },
];

describe('joinNames', () => {
  it('joins short lists naturally', () => {
    expect(joinNames([])).toBe('');
    expect(joinNames(['Ann'])).toBe('Ann');
    expect(joinNames(['Ann', 'Bo'])).toBe('Ann and Bo');
    expect(joinNames(['Ann', 'Bo', 'Cy'])).toBe('Ann, Bo and Cy');
  });

  it('collapses the tail of a long list into a count', () => {
    const names = Array.from({ length: 13 }, (_, i) => `U${i}`);
    expect(joinNames(names)).toBe('U0, U1, U2, U3, U4, U5, U6, U7, U8, U9 and 3 more');
  });
});

describe('reactionTabs and reactorRows', () => {
  it('starts with an All tab counting every reaction', () => {
    expect(reactionTabs(reactions)).toEqual([
      { key: null, count: 3 },
      { key: '\u{1F44D}', count: 2 },
      { key: '\u{2764}\u{FE0F}', count: 1 },
    ]);
  });

  it('lists one row per reaction, or only the chosen emoji', () => {
    expect(reactorRows(reactions, null)).toHaveLength(3);
    expect(reactorRows(reactions, '\u{2764}\u{FE0F}')).toEqual([{ userId: '@b:hs', key: '\u{2764}\u{FE0F}' }]);
  });
});
