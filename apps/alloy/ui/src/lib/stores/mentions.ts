import type { RoomMemberDto, UserId } from '../generated/index.js';
import { memberLabel } from './members.svelte.js';

/** An `@` token being typed: `start` indexes the `@`, `query` is the text after it. */
export interface ActiveMention {
  start: number;
  query: string;
}

/** A mention picked in the composer: the label inserted into the text and the user it points at. */
export interface MentionRecord {
  userId: UserId;
  label: string;
}

/** Most suggestions the picker shows at once. */
export const MENTION_LIMIT = 8;

// `@` at the start of the text or after whitespace, followed by non-space characters up to the caret.
// The `@` in `a@b.com` is preceded by a letter, so it never matches.
const TOKEN_AT_CARET = /(^|\s)@(\S*)$/;

/** The `@query` token ending at `caret`, or null when the caret is not inside one. */
export function activeMention(text: string, caret: number): ActiveMention | null {
  if (caret < 0 || caret > text.length) return null;
  const match = TOKEN_AT_CARET.exec(text.slice(0, caret));
  if (!match) return null;
  return { start: match.index + match[1].length, query: match[2] };
}

const MEMBERSHIP_ORDER: Partial<Record<RoomMemberDto['membership'], number>> = { Joined: 0, Invited: 1 };

/** 0 = name or localpart starts with the query, 1 = name contains it, 2 = user id contains it. */
function matchRank(member: RoomMemberDto, query: string): number | null {
  if (query === '') return 0;
  const label = memberLabel(member).toLowerCase();
  const localpart = member.user_id.replace(/^@/, '').split(':')[0].toLowerCase();
  if (label.startsWith(query) || localpart.startsWith(query)) return 0;
  if (label.includes(query)) return 1;
  if (member.user_id.toLowerCase().includes(query)) return 2;
  return null;
}

/**
 * Members to suggest for `query`: joined members before invited ones, then by match quality,
 * then by label. Left and banned members are never suggested.
 */
export function rankMembers(
  members: readonly RoomMemberDto[],
  query: string,
  limit: number
): RoomMemberDto[] {
  const q = query.trim().toLowerCase();
  const ranked: Array<{ member: RoomMemberDto; membership: number; rank: number; label: string }> = [];
  for (const member of members) {
    const membership = MEMBERSHIP_ORDER[member.membership];
    if (membership === undefined) continue;
    const rank = matchRank(member, q);
    if (rank === null) continue;
    ranked.push({ member, membership, rank, label: memberLabel(member).toLowerCase() });
  }
  ranked.sort(
    (a, b) =>
      a.membership - b.membership ||
      a.rank - b.rank ||
      a.label.localeCompare(b.label) ||
      a.member.user_id.localeCompare(b.member.user_id)
  );
  return ranked.slice(0, Math.max(0, limit)).map((r) => r.member);
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Whether `@label` still appears as a whole mention: at a word start and not followed by more name characters. */
function mentionPresent(text: string, label: string): boolean {
  if (label === '') return false;
  const pattern = new RegExp(`(?<!\\S)@${escapeRegExp(label)}(?![\\p{L}\\p{N}_])`, 'u');
  return pattern.test(text);
}

/**
 * The user ids of recorded mentions whose `@Label` still occurs in `text`.
 * A mention the user deleted (or edited away) is dropped; each user appears once.
 */
export function reconcileMentions(text: string, recorded: readonly MentionRecord[]): UserId[] {
  const out: UserId[] = [];
  for (const { userId, label } of recorded) {
    if (out.includes(userId)) continue;
    if (mentionPresent(text, label)) out.push(userId);
  }
  return out;
}

/** DOM id of the picker option at `index`, for `aria-activedescendant`. */
export function mentionOptionId(listId: string, index: number): string {
  return `${listId}-option-${index}`;
}
