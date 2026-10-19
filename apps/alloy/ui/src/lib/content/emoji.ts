// User-facing emoji content: the shared picker's data helpers. This is
// content, not UI chrome, so it is exempt from the emoji guard (see emoji-guard.test.ts).
// Picker data comes from emojibase-data (MIT), loaded lazily the first time the picker opens.

export type SkinTone = 'none' | 'light' | 'medium-light' | 'medium' | 'medium-dark' | 'dark';

/** Fitzpatrick modifier code points, keyed by tone. `none` is the default yellow. */
const TONE_MODIFIER_HEX: Record<Exclude<SkinTone, 'none'>, string> = {
  light: '1F3FB',
  'medium-light': '1F3FC',
  medium: '1F3FD',
  'medium-dark': '1F3FE',
  dark: '1F3FF',
};

const SKIN_MODIFIER_RE = /1F3F[B-F]/g;

export interface SkinToneOption {
  id: SkinTone;
  label: string;
  /** A waving hand in this tone, for the picker's tone button. */
  swatch: string;
}

const WAVING_HAND = 0x1f44b;

export const SKIN_TONES: SkinToneOption[] = [
  { id: 'none', label: 'Default', swatch: String.fromCodePoint(WAVING_HAND) },
  { id: 'light', label: 'Light', swatch: String.fromCodePoint(WAVING_HAND, 0x1f3fb) },
  { id: 'medium-light', label: 'Medium-light', swatch: String.fromCodePoint(WAVING_HAND, 0x1f3fc) },
  { id: 'medium', label: 'Medium', swatch: String.fromCodePoint(WAVING_HAND, 0x1f3fd) },
  { id: 'medium-dark', label: 'Medium-dark', swatch: String.fromCodePoint(WAVING_HAND, 0x1f3fe) },
  { id: 'dark', label: 'Dark', swatch: String.fromCodePoint(WAVING_HAND, 0x1f3ff) },
];

export function isSkinTone(value: unknown): value is SkinTone {
  return SKIN_TONES.some((t) => t.id === value);
}

/** Entry shape of emojibase-data `compact.json`; only the fields the picker reads are typed. */
export interface RawEmoji {
  hexcode: string;
  label: string;
  unicode: string;
  /** Index into `messages.json` groups. Absent for regional indicator letters. */
  group?: number;
  order?: number;
  tags?: string[];
  skins?: RawEmoji[];
}

export interface RawGroup {
  key: string;
  message: string;
  order: number;
}

export interface EmojiEntry {
  emoji: string;
  label: string;
  hexcode: string;
  tags: string[];
  order: number;
  /** Skin-tone variants of this emoji, empty when it has none. */
  skins: EmojiEntry[];
}

export interface EmojiGroup {
  key: string;
  label: string;
  emojis: EmojiEntry[];
}

export interface EmojiData {
  groups: EmojiGroup[];
  /** Every grouped emoji in display order, without skin variants. */
  all: EmojiEntry[];
}

const COMPONENT_GROUP = 'component';

function toEntry(raw: RawEmoji): EmojiEntry {
  return {
    emoji: raw.unicode,
    label: raw.label,
    hexcode: raw.hexcode,
    tags: raw.tags ?? [],
    order: raw.order ?? 0,
    skins: (raw.skins ?? []).map(toEntry),
  };
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0].toUpperCase() + s.slice(1) : s;
}

/**
 * Groups raw emoji by their emojibase group, in group order and each group sorted by `order`.
 * The component group (skin-tone swatches and hair styles) is not browsable and is dropped, as are
 * entries without a group.
 */
export function groupEmoji(raw: RawEmoji[], groups: RawGroup[]): EmojiGroup[] {
  const byIndex = new Map(groups.map((g) => [g.order, g] as const));
  const buckets = new Map<string, EmojiEntry[]>();
  for (const e of raw) {
    const g = e.group === undefined ? undefined : byIndex.get(e.group);
    if (!g || g.key === COMPONENT_GROUP) continue;
    const list = buckets.get(g.key) ?? [];
    list.push(toEntry(e));
    buckets.set(g.key, list);
  }
  return groups
    .filter((g) => g.key !== COMPONENT_GROUP)
    .sort((a, b) => a.order - b.order)
    .flatMap((g) => {
      const emojis = buckets.get(g.key);
      if (!emojis || emojis.length === 0) return [];
      emojis.sort((a, b) => a.order - b.order);
      return [{ key: g.key, label: capitalize(g.message), emojis }];
    });
}

export function buildEmojiData(raw: RawEmoji[], groups: RawGroup[]): EmojiData {
  const grouped = groupEmoji(raw, groups);
  return { groups: grouped, all: grouped.flatMap((g) => g.emojis) };
}

/** Lower-cases and strips diacritics so that "Café" matches "cafe". */
export function normalizeText(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/** Turns a typed query into a plain phrase: ":thumbs_up:" becomes "thumbs up". */
export function searchQuery(query: string): string {
  return normalizeText(query)
    .trim()
    .replace(/^:+|:+$/g, '')
    .replace(/[_\s]+/g, ' ')
    .trim();
}

const EXACT = 0;
const PREFIX = 1;
const WORD_PREFIX = 2;
const CONTAINS = 3;

/** How well `text` matches the normalised query `q`, lower is better; null when it does not match. */
function rank(text: string, q: string): number | null {
  if (text === q) return EXACT;
  if (text.startsWith(q)) return PREFIX;
  if (text.split(/[\s_-]+/).some((w) => w.startsWith(q))) return WORD_PREFIX;
  if (text.includes(q)) return CONTAINS;
  return null;
}

/**
 * Emoji whose label or tags match `query`, best match first (exact, prefix, word prefix, contains),
 * ties broken by emojibase order. Empty for an empty query.
 */
export function searchEmoji(all: EmojiEntry[], query: string, limit = 80): EmojiEntry[] {
  const q = searchQuery(query);
  if (q === '') return [];
  const hits: { entry: EmojiEntry; rank: number }[] = [];
  for (const entry of all) {
    let best = rank(normalizeText(entry.label), q) ?? Infinity;
    for (const tag of entry.tags) {
      const r = rank(normalizeText(tag), q);
      if (r !== null && r < best) best = r;
    }
    if (best !== Infinity) hits.push({ entry, rank: best });
  }
  hits.sort((a, b) => a.rank - b.rank || a.entry.order - b.entry.order);
  return hits.slice(0, limit).map((h) => h.entry);
}

/** The variant of `entry` in `tone`, or the entry itself when it has no such variant. */
export function applySkinTone(entry: EmojiEntry, tone: SkinTone): EmojiEntry {
  if (tone === 'none' || entry.skins.length === 0) return entry;
  const want = TONE_MODIFIER_HEX[tone];
  const match = entry.skins.find((s) => {
    const modifiers = s.hexcode.match(SKIN_MODIFIER_RE);
    return modifiers !== null && modifiers.every((m) => m === want);
  });
  return match ?? entry;
}

let dataPromise: Promise<EmojiData> | null = null;

/** Loads the picker data once; the JSON chunks are fetched on first call only. */
export function loadEmojiData(): Promise<EmojiData> {
  dataPromise ??= Promise.all([
    import('emojibase-data/en/compact.json'),
    import('emojibase-data/en/messages.json'),
  ])
    .then(([compact, messages]) =>
      buildEmojiData(compact.default as RawEmoji[], (messages.default as { groups: RawGroup[] }).groups)
    )
    .catch((err: unknown) => {
      dataPromise = null;
      throw err;
    });
  return dataPromise;
}
