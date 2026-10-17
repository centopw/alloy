/**
 * Extracts 1-2 initials from a display name or fallback Matrix user ID.
 * Handles emoji (strips decorative emoji), CJK characters, and @id fallbacks.
 */

// Matches Unicode Extended Pictographic emoji + skin tones + joiners
const EMOJI_REGEX = /[\p{Extended_Pictographic}\u{1F3FB}-\u{1F3FF}\u{FE0F}\u{200D}]/gu;

// Matches CJK ideographs, hiragana, katakana, hangul
const CJK_REGEX = /[\u4e00-\u9fff\u3400-\u4dbf\u3040-\u30ff\uac00-\ud7af]/;

/**
 * Derives 1-2 initials from display name or fallback ID.
 */
export function getInitials(displayName?: string | null, fallbackId?: string | null): string {
  // Strip emoji from display name
  let clean = (displayName || '').replace(EMOJI_REGEX, '').trim();

  // If display name is empty or only emoji, fall back to matrix ID
  if (!clean && fallbackId) {
    let local = fallbackId.trim();
    if (local.startsWith('@')) {
      local = local.slice(1);
    }
    const colonIdx = local.indexOf(':');
    if (colonIdx !== -1) {
      local = local.slice(0, colonIdx);
    }
    clean = local.replace(EMOJI_REGEX, '').trim();
  }

  if (!clean) return '?';

  // Handle CJK characters: return first 1-2 CJK characters
  if (CJK_REGEX.test(clean)) {
    const chars = Array.from(clean).filter((c) => !/\s/.test(c));
    if (chars.length >= 2) {
      return chars.slice(0, 2).join('');
    }
    return chars[0] || '?';
  }

  // Split into words
  const words = clean.split(/\s+/).filter((w) => w.length > 0);
  if (words.length === 0) return '?';

  if (words.length === 1) {
    const word = words[0];
    const letters = Array.from(word).filter((c) => /\p{L}|\p{N}/u.test(c));
    if (letters.length >= 2) {
      return (letters[0] + letters[1]).toUpperCase();
    }
    return (letters[0] || word[0] || '?').toUpperCase();
  }

  // Multi-word: first letter of first word + first letter of last word
  const first = Array.from(words[0]).find((c) => /\p{L}|\p{N}/u.test(c)) || words[0][0];
  const last =
    Array.from(words[words.length - 1]).find((c) => /\p{L}|\p{N}/u.test(c)) ||
    words[words.length - 1][0];

  return (first + last).toUpperCase();
}
