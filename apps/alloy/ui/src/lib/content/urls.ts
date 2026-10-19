import { parseMatrixToLink } from '../platform/matrixTo.js';

/** Fenced blocks and inline code spans never produce previews. */
const CODE_SPAN = /```[\s\S]*?```|`[^`\n]*`/g;
const HTTP_URL = /https?:\/\/[^\s<>"'`]+/gi;
/** Sentence punctuation that follows a URL in prose, not part of it. */
const TRAILING_PUNCTUATION = /[.,;:!?)\]}]+$/;

/**
 * First http(s) URL in a plain-text message body that could carry a link preview.
 * Code is ignored; matrix.to permalinks are ignored because they open in-app.
 */
export function firstPreviewUrl(body: string): string | null {
  const prose = body.replace(CODE_SPAN, ' ');
  for (const match of prose.matchAll(HTTP_URL)) {
    const url = match[0].replace(TRAILING_PUNCTUATION, '');
    if (parseMatrixToLink(url) === null) return url;
  }
  return null;
}
