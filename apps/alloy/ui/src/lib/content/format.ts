import { prefs, type TimeFormat } from '../stores/prefs.svelte.js';

function hour12Option(pref: TimeFormat): boolean | undefined {
  if (pref === '12h') return true;
  if (pref === '24h') return false;
  return undefined;
}

/** Clock time; `auto` follows the system locale, `12h`/`24h` force the clock. */
export function formatTime(ms: number, pref: TimeFormat = prefs.timeFormat): string {
  return new Date(ms).toLocaleTimeString([], {
    hour: pref === '24h' ? '2-digit' : 'numeric',
    minute: '2-digit',
    hour12: hour12Option(pref),
  });
}

export function formatDay(ms: number, now = new Date()): string {
  const d = new Date(ms);
  const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (same(d, now)) return 'Today';
  if (same(d, yesterday)) return 'Yesterday';
  return d.toLocaleDateString([], {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
    year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric',
  });
}

/** Relative stamp for room list rows: time today, weekday this week, else date. */
export function formatListTime(ms: number, now = new Date(), pref: TimeFormat = prefs.timeFormat): string {
  const d = new Date(ms);
  if (d.toDateString() === now.toDateString()) return formatTime(ms, pref);
  const days = (now.getTime() - ms) / 86_400_000;
  if (days < 6) return d.toLocaleDateString([], { weekday: 'short' });
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
const EMOJI_PART = /\p{Extended_Pictographic}|\p{Regional_Indicator}|\p{Emoji_Modifier}|\u20e3/u;
const WHITESPACE = /^\s+$/u;

/**
 * True when `text` is only 1..maxCount emoji (whitespace between them allowed).
 * Multi-codepoint sequences (ZWJ families, flags, skin tones, keycaps) count as one emoji.
 */
export function isEmojiOnly(text: string, maxCount = 3): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;
  let count = 0;
  for (const { segment } of graphemes.segment(trimmed)) {
    if (WHITESPACE.test(segment)) continue;
    if (!EMOJI_PART.test(segment)) return false;
    count += 1;
    if (count > maxCount) return false;
  }
  return count > 0;
}

/**
 * Whether a media message's `body` is just the file name. Clients without captions put the file
 * name there, so it is not a caption worth showing under the picture.
 */
export function isFileName(body: string): boolean {
  return /^\S+\.[A-Za-z0-9]{2,5}$/.test(body.trim());
}
