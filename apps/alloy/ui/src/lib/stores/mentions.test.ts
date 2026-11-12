import { describe, it, expect } from 'vitest';
import type { RoomMemberDto } from '../generated/index.js';
import {
  activeMention,
  mentionOptionId,
  mentionsAny,
  rankMembers,
} from './mentions.js';

function member(user_id: string, display_name: string | null, role: string | null = null): RoomMemberDto {
  return { user_id, display_name, avatar: null, role };
}

describe('activeMention', () => {
  it('detects a query mid-text up to the caret', () => {
    expect(activeMention('hi @bob there', 7)).toEqual({ start: 3, query: 'bob' });
  });

  it('returns null once the caret leaves the token', () => {
    expect(activeMention('hi @bob there', 13)).toBeNull();
  });

  it('opens with an empty query on a bare @', () => {
    expect(activeMention('@', 1)).toEqual({ start: 0, query: '' });
  });

  it('accepts an @ after a newline', () => {
    expect(activeMention('x\n@ann', 6)).toEqual({ start: 2, query: 'ann' });
  });

  it('ignores an @ inside an email address', () => {
    expect(activeMention('mail me@bob.com', 16)).toBeNull();
  });

  it('closes when whitespace follows the query', () => {
    expect(activeMention('@ann x', 6)).toBeNull();
  });

  it('rejects out-of-range carets', () => {
    expect(activeMention('@ann', -1)).toBeNull();
    expect(activeMention('@ann', 9)).toBeNull();
  });
});

describe('rankMembers', () => {
  const alice = member('u-alice', 'Alice');
  const alicia = member('u-alicia', 'Alicia');

  it('matches case-insensitively on display name', () => {
    expect(rankMembers([alice], 'ALI', 8)).toEqual([alice]);
  });

  it('matches on user id when the display name does not match', () => {
    const zed = member('zed-9', null);
    expect(rankMembers([zed], 'zed', 8)).toEqual([zed]);
  });

  it('ranks name-prefix matches above substring matches', () => {
    const prefix = member('u-alex', 'Alex');
    const substring = member('u-bob', 'Bob Alex');
    expect(rankMembers([substring, prefix], 'al', 8).map((m) => m.user_id)).toEqual([
      'u-alex',
      'u-bob'
    ]);
  });

  it('surfaces a name match before a user-id-only match', () => {
    const byId = member('al-in-the-id', 'Zoe');
    expect(rankMembers([byId, alice], 'al', 8).map((m) => m.user_id)).toEqual([
      'u-alice',
      'al-in-the-id'
    ]);
  });

  it('returns every member for an empty query, sorted by label', () => {
    const b = member('u-b', 'Bea');
    const a = member('u-a', 'Ann');
    expect(rankMembers([b, a], '', 8).map((m) => m.user_id)).toEqual(['u-a', 'u-b']);
  });

  it('skips members that do not match', () => {
    expect(rankMembers([alice, alicia], 'zz', 8)).toEqual([]);
  });

  it('caps the result at the limit', () => {
    const many = Array.from({ length: 12 }, (_, i) => member(`u${i}`, `User ${i}`));
    expect(rankMembers(many, '', 8)).toHaveLength(8);
  });
});

describe('mentionsAny', () => {
  it('matches a display name typed as an @Label token', () => {
    expect(mentionsAny('hey @Mara Vogel, ping', ['Mara Vogel'])).toBe(true);
  });

  it('requires a whole token: a longer name or a prefixed word does not count', () => {
    expect(mentionsAny('@Mara Vogels hi', ['Mara Vogel'])).toBe(false);
    expect(mentionsAny('mail@Mara Vogel', ['Mara Vogel'])).toBe(false);
  });

  it('reports nothing without a name', () => {
    expect(mentionsAny('@Mara hi', [])).toBe(false);
    expect(mentionsAny('@Mara hi', [''])).toBe(false);
  });
});

describe('mentionOptionId', () => {
  it('builds stable option ids for aria-activedescendant', () => {
    expect(mentionOptionId('mentions-1', 2)).toBe('mentions-1-option-2');
  });
});
