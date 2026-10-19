import type { ReactionSummary, UserId } from '$lib/generated';

const MAX_NAMES = 10;

/** "A, B and 3 more": at most `max` names, then a count of the rest. */
export function joinNames(names: readonly string[], max = MAX_NAMES): string {
  if (names.length <= max) {
    if (names.length <= 1) return names.join('');
    return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
  }
  const rest = names.length - max;
  return `${names.slice(0, max).join(', ')} and ${rest} more`;
}

export interface ReactionTab {
  /** `null` is the "All" tab. */
  key: string | null;
  count: number;
}

/** Tabs for the who-reacted sheet: All, then one per emoji in message order. */
export function reactionTabs(reactions: readonly ReactionSummary[]): ReactionTab[] {
  const total = reactions.reduce((n, r) => n + r.count, 0);
  return [{ key: null, count: total }, ...reactions.map((r) => ({ key: r.key, count: r.count }))];
}

export interface ReactorRow {
  userId: UserId;
  key: string;
}

/** One row per (user, emoji) for the selected tab; `key === null` lists every reaction. */
export function reactorRows(reactions: readonly ReactionSummary[], key: string | null): ReactorRow[] {
  return reactions
    .filter((r) => key === null || r.key === key)
    .flatMap((r) => r.senders.map((userId) => ({ userId, key: r.key })));
}
