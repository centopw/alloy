/** @module tokens - token list parsing. A token maps to a byte quota. */

/** Default quota per token when the config does not name one: 256 MiB. */
export const DEFAULT_QUOTA_BYTES = 256 * 1024 * 1024;

/**
 * Parse a token specification: entries separated by commas or newlines, each
 * `token` or `token:quota_bytes`. Blank lines and `#` comments are ignored, so the
 * same parser reads SYNC_TOKENS and a tokens file.
 *
 * A token itself is compared verbatim; the trailing `:quota` is only split off when
 * the suffix parses as a positive integer.
 *
 * @param {string | null | undefined} text
 * @returns {{ token: string, quota: number }[]}
 */
export function parseTokenSpec(text) {
  /** @type {{ token: string, quota: number }[]} */
  const tokens = [];
  for (const line of String(text ?? '').split(/[\n,]/)) {
    const entry = line.trim();
    if (!entry || entry.startsWith('#')) continue;
    const separator = entry.lastIndexOf(':');
    if (separator > 0) {
      const quota = Number(entry.slice(separator + 1));
      if (Number.isSafeInteger(quota) && quota > 0) {
        tokens.push({ token: entry.slice(0, separator), quota });
        continue;
      }
    }
    tokens.push({ token: entry, quota: DEFAULT_QUOTA_BYTES });
  }
  return tokens;
}
