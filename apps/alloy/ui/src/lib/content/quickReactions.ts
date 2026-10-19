/** Most reactions the quick bar holds; beyond this the bar stops fitting a phone sheet. */
export const MAX_QUICK_REACTIONS = 8;
/** Longest accepted emoji string (a ZWJ sequence with modifiers stays well under this). */
const MAX_EMOJI_LENGTH = 32;

/** The reactions a new account starts with: thumbs up, heart, laughing, surprised, sad, folded hands. */
export const DEFAULT_QUICK_REACTIONS: readonly string[] = Object.freeze([
  '\u{1F44D}',
  '\u2764\uFE0F',
  '\u{1F602}',
  '\u{1F62E}',
  '\u{1F622}',
  '\u{1F64F}',
]);

/** Whether `value` is a usable quick-reaction list: 1 to MAX distinct non-empty short strings. */
export function isQuickReactions(value: unknown): value is string[] {
  return (
    Array.isArray(value) &&
    value.length >= 1 &&
    value.length <= MAX_QUICK_REACTIONS &&
    value.every((e) => typeof e === 'string' && e.length > 0 && e.length <= MAX_EMOJI_LENGTH) &&
    new Set(value).size === value.length
  );
}

/** Appends `emoji` unless it is already there or the bar is full. Returns the same list when nothing changes. */
export function addQuickReaction(list: readonly string[], emoji: string): readonly string[] {
  if (list.includes(emoji) || list.length >= MAX_QUICK_REACTIONS) return list;
  return [...list, emoji];
}

/** Drops `emoji`. The last remaining reaction cannot be removed, so the bar never ends up empty. */
export function removeQuickReaction(list: readonly string[], emoji: string): readonly string[] {
  if (list.length <= 1 || !list.includes(emoji)) return list;
  return list.filter((e) => e !== emoji);
}
