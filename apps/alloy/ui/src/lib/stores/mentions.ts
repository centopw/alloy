import type { RoomMemberDto } from '../generated/index.js';
import { memberLabel } from './members.svelte.js';

/** An `@` token being typed: `start` indexes the `@`, `query` is the text after it. */
export interface ActiveMention {
  start: number;
  query: string;
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

/** 0 = name starts with the query, 1 = name contains it, 2 = user id contains it. */
function matchRank(member: RoomMemberDto, query: string): number | null {
  if (query === '') return 0;
  const label = memberLabel(member).toLowerCase();
  if (label.startsWith(query)) return 0;
  if (label.includes(query)) return 1;
  if (member.user_id.toLowerCase().includes(query)) return 2;
  return null;
}

/** Members to suggest for `query`: by match quality, then by label. */
export function rankMembers(
  members: readonly RoomMemberDto[],
  query: string,
  limit: number
): RoomMemberDto[] {
  const q = query.trim().toLowerCase();
  const ranked: Array<{ member: RoomMemberDto; rank: number; label: string }> = [];
  for (const member of members) {
    const rank = matchRank(member, q);
    if (rank === null) continue;
    ranked.push({ member, rank, label: memberLabel(member).toLowerCase() });
  }
  ranked.sort(
    (a, b) =>
      a.rank - b.rank ||
      a.label.localeCompare(b.label) ||
      a.member.user_id.localeCompare(b.member.user_id)
  );
  return ranked.slice(0, Math.max(0, limit)).map((r) => r.member);
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Whether `@label` appears as a whole mention: at a word start and not followed by more name
 * characters. Mentions are plain text on every v2 network, so the label is the only marker.
 */
export function containsMention(text: string, label: string): boolean {
  if (label === '') return false;
  const pattern = new RegExp(`(?<!\\S)@${escapeRegExp(label)}(?![\\p{L}\\p{N}_])`, 'u');
  return pattern.test(text);
}

/** Whether any of the signed-in user's names is mentioned as a whole `@Label` token. */
export function mentionsAny(text: string, labels: readonly string[]): boolean {
  return labels.some((label) => containsMention(text, label));
}

/** DOM id of the picker option at `index`, for `aria-activedescendant`. */
export function mentionOptionId(listId: string, index: number): string {
  return `${listId}-option-${index}`;
}
