import { isSkinTone, type SkinTone } from '$lib/content/emoji';

export const RECENT_EMOJI_KEY = 'alloy.emojiRecent';
export const EMOJI_TONE_KEY = 'alloy.emojiTone';
export const MAX_RECENT_EMOJI = 32;

export interface RecentEmoji {
  emoji: string;
  count: number;
}

type ReadStorage = Pick<Storage, 'getItem'>;
type WriteStorage = Pick<Storage, 'setItem'>;

function browserStorage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

/** Parses stored recents, dropping anything malformed. Corrupt storage yields an empty list. */
export function loadRecents(storage: ReadStorage | null): RecentEmoji[] {
  let raw: string | null;
  try {
    raw = storage?.getItem(RECENT_EMOJI_KEY) ?? null;
  } catch {
    return [];
  }
  if (!raw) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const seen = new Set<string>();
  const out: RecentEmoji[] = [];
  for (const item of parsed as unknown[]) {
    if (typeof item !== 'object' || item === null) continue;
    const { emoji, count } = item as { emoji?: unknown; count?: unknown };
    if (typeof emoji !== 'string' || emoji === '' || seen.has(emoji)) continue;
    if (typeof count !== 'number' || !Number.isInteger(count) || count < 1) continue;
    seen.add(emoji);
    out.push({ emoji, count });
    if (out.length === MAX_RECENT_EMOJI) break;
  }
  return out;
}

export function saveRecents(storage: WriteStorage | null, recents: RecentEmoji[]): void {
  try {
    storage?.setItem(RECENT_EMOJI_KEY, JSON.stringify(recents));
  } catch {
    // Quota or privacy mode: recents are a convenience, so dropping them is acceptable.
  }
}

/** Moves `emoji` to the front, bumping its count, and caps the list at `MAX_RECENT_EMOJI`. */
export function touchRecent(recents: RecentEmoji[], emoji: string): RecentEmoji[] {
  const previous = recents.find((r) => r.emoji === emoji);
  const next: RecentEmoji = { emoji, count: (previous?.count ?? 0) + 1 };
  return [next, ...recents.filter((r) => r.emoji !== emoji)].slice(0, MAX_RECENT_EMOJI);
}

export function loadSkinTone(storage: ReadStorage | null): SkinTone {
  try {
    const raw = storage?.getItem(EMOJI_TONE_KEY) ?? null;
    return isSkinTone(raw) ? raw : 'none';
  } catch {
    return 'none';
  }
}

export function saveSkinTone(storage: WriteStorage | null, tone: SkinTone): void {
  try {
    storage?.setItem(EMOJI_TONE_KEY, tone);
  } catch {
    // Same as recents: losing the preference is harmless.
  }
}

let recents: RecentEmoji[] | null = null;
let tone: SkinTone | null = null;

/** Recent emoji, most recently used first. Returns a copy. */
export function recentEmoji(): RecentEmoji[] {
  recents ??= loadRecents(browserStorage());
  return recents.slice();
}

export function recordEmoji(emoji: string): void {
  recents = touchRecent(recentEmoji(), emoji);
  saveRecents(browserStorage(), recents);
}

export function getSkinTone(): SkinTone {
  tone ??= loadSkinTone(browserStorage());
  return tone;
}

export function setSkinTone(next: SkinTone): void {
  tone = next;
  saveSkinTone(browserStorage(), next);
}
