/**
 * Matching for in-room message search. Matching ignores case and accents, and highlight runs map
 * back onto the original text so they slice it exactly. Output is plain runs, never HTML: callers
 * render each run as text.
 */

export interface TextRun {
  text: string;
  match: boolean;
}

/** Lowercases and strips combining marks, so "Café" matches "cafe". */
export function normalizeForSearch(text: string): string {
  return text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/** True when `text` contains `query` under search normalization. A blank query matches nothing. */
export function matchesQuery(text: string, query: string): boolean {
  const needle = normalizeForSearch(query.trim());
  if (!needle) return false;
  return normalizeForSearch(text).includes(needle);
}

/**
 * Splits `text` into runs marking each occurrence of `query`. Matches are found in the normalized
 * text, then mapped back to UTF-16 offsets in the original so every run is an exact slice of it.
 */
export function highlightRuns(text: string, query: string): TextRun[] {
  const needle = normalizeForSearch(query.trim());
  if (!needle) return [{ text, match: false }];

  // Normalize one code point at a time, recording the original span each normalized unit came from.
  let norm = '';
  const starts: number[] = [];
  const ends: number[] = [];
  let index = 0;
  for (const ch of text) {
    const end = index + ch.length;
    const folded = normalizeForSearch(ch);
    for (let i = 0; i < folded.length; i++) {
      starts.push(index);
      ends.push(end);
    }
    norm += folded;
    index = end;
  }

  const ranges: Array<[number, number]> = [];
  for (let at = norm.indexOf(needle); at !== -1; at = norm.indexOf(needle, at + needle.length)) {
    ranges.push([starts[at], ends[at + needle.length - 1]]);
  }
  if (ranges.length === 0) return [{ text, match: false }];

  const runs: TextRun[] = [];
  let cursor = 0;
  for (const [start, end] of ranges) {
    if (start > cursor) runs.push({ text: text.slice(cursor, start), match: false });
    runs.push({ text: text.slice(start, end), match: true });
    cursor = end;
  }
  if (cursor < text.length) runs.push({ text: text.slice(cursor), match: false });
  return runs;
}
