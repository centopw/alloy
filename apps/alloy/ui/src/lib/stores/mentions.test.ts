import { describe, it, expect } from 'vitest';
import type { RoomMemberDto } from '../generated/index.js';
import {
  activeMention,
  mentionOptionId,
  rankMembers,
  roomMentionOption,
  containsRoomMention,
  reconcileMentions,
  type MentionRecord
} from './mentions.js';

function member(
  user_id: string,
  display_name: string | null,
  membership: RoomMemberDto['membership'] = 'Joined'
): RoomMemberDto {
  return { user_id, display_name, avatar_url: null, power_level: 0, membership };
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
    expect(activeMention('mail a@b.com', 12)).toBeNull();
    expect(activeMention('a@b', 3)).toBeNull();
  });

  it('closes when whitespace follows the query', () => {
    expect(activeMention('@ann x', 6)).toBeNull();
  });

  it('rejects out-of-range carets', () => {
    expect(activeMention('@a', -1)).toBeNull();
    expect(activeMention('@a', 3)).toBeNull();
  });
});

describe('rankMembers', () => {
  const alice = member('@alice:hs', 'Alice', 'Joined');
  const alicia = member('@alicia:hs', 'Alicia', 'Joined');
  const invitedAli = member('@ali:hs', 'Ali Invited', 'Invited');

  it('lists joined members before invited ones', () => {
    const ranked = rankMembers([invitedAli, alicia, alice], 'ali', 8);
    expect(ranked.map((m) => m.user_id)).toEqual(['@alice:hs', '@alicia:hs', '@ali:hs']);
  });

  it('excludes left and banned members', () => {
    const left = member('@gone:hs', 'Gone', 'Left');
    const banned = member('@bad:hs', 'Gone Too', 'Banned');
    expect(rankMembers([left, banned, alice], '', 8).map((m) => m.user_id)).toEqual(['@alice:hs']);
  });

  it('matches case-insensitively on display name', () => {
    expect(rankMembers([alice], 'ALI', 8)).toEqual([alice]);
  });

  it('matches on user id when the display name does not match', () => {
    const zed = member('@zed:hs', null);
    expect(rankMembers([zed], 'zed', 8)).toEqual([zed]);
  });

  it('ranks name-prefix matches above substring matches within the same membership', () => {
    const prefix = member('@alex:hs', 'Alex');
    const substring = member('@bob:hs', 'Bob Alex');
    expect(rankMembers([substring, prefix], 'al', 8).map((m) => m.user_id)).toEqual([
      '@alex:hs',
      '@bob:hs'
    ]);
  });

  it('returns every joined member for an empty query, sorted by label', () => {
    const b = member('@b:hs', 'Bea');
    const a = member('@a:hs', 'Ann');
    expect(rankMembers([b, a], '', 8).map((m) => m.user_id)).toEqual(['@a:hs', '@b:hs']);
  });

  it('caps the result at the limit', () => {
    const many = Array.from({ length: 12 }, (_, i) => member(`@u${i}:hs`, `User ${i}`));
    expect(rankMembers(many, '', 8)).toHaveLength(8);
  });
});

describe('reconcileMentions', () => {
  const bob: MentionRecord = { userId: '@bob:hs', label: 'Bob Smith' };

  it('keeps a mention whose label is still in the text', () => {
    expect(reconcileMentions('hi @Bob Smith ', [bob])).toEqual(['@bob:hs']);
  });

  it('drops a mention the user deleted', () => {
    expect(reconcileMentions('hi there', [bob])).toEqual([]);
  });

  it('drops a mention the user edited away to a partial label', () => {
    expect(reconcileMentions('hi @Bob', [bob])).toEqual([]);
  });

  it('does not treat a longer name as the recorded label', () => {
    const short: MentionRecord = { userId: '@bob:hs', label: 'Bob' };
    expect(reconcileMentions('@Bobby hi', [short])).toEqual([]);
  });

  it('requires the label to start at a word boundary', () => {
    const short: MentionRecord = { userId: '@bob:hs', label: 'Bob' };
    expect(reconcileMentions('mail@Bob', [short])).toEqual([]);
  });

  it('matches labels containing regex metacharacters literally', () => {
    const odd: MentionRecord = { userId: '@odd:hs', label: 'A.B (x)' };
    expect(reconcileMentions('@A.B (x) ok', [odd])).toEqual(['@odd:hs']);
    expect(reconcileMentions('@AxB (x) ok', [odd])).toEqual([]);
  });

  it('lists each user once even if recorded twice', () => {
    expect(reconcileMentions('@Bob Smith @Bob Smith', [bob, bob])).toEqual(['@bob:hs']);
  });
});

describe('mentionOptionId', () => {
  it('builds stable option ids for aria-activedescendant', () => {
    expect(mentionOptionId('mentions-1', 2)).toBe('mentions-1-option-2');
  });
});

describe('@room', () => {
  it('suggests @room only while the query can still become "room" and the user may use it', () => {
    expect(roomMentionOption('', true)?.user_id).toBe('@room');
    expect(roomMentionOption('ro', true)?.user_id).toBe('@room');
    expect(roomMentionOption('bob', true)).toBeNull();
    expect(roomMentionOption('', false)).toBeNull();
  });

  it('finds a whole-word @room in the text', () => {
    expect(containsRoomMention('hey @room, lunch?')).toBe(true);
    expect(containsRoomMention('@room')).toBe(true);
    expect(containsRoomMention('mail me@room.com')).toBe(false);
    expect(containsRoomMention('@rooms')).toBe(false);
  });
});
