/** A `matrix.to` permalink target: a user (`@id`) or a room (`!id` or `#alias`). */
export type MatrixToLink = { kind: 'user' | 'room'; id: string };

const ID_PATTERN = /^([@!#])[^\s/:]+:[^\s/]+$/;

/**
 * Parse a `https://matrix.to/#/…` permalink. Returns null for any other host, scheme or target.
 * Event permalinks (`/#/!room/$event`) resolve to their room. Pure: no DOM or store access.
 */
export function parseMatrixToLink(href: string): MatrixToLink | null {
  let url: URL;
  try {
    url = new URL(href.trim());
  } catch {
    return null;
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') return null;
  if (url.hostname.toLowerCase() !== 'matrix.to') return null;
  if (!url.hash.startsWith('#/')) return null;

  let target: string;
  try {
    target = decodeURIComponent(url.hash.slice(2));
  } catch {
    return null;
  }
  // Drop query parameters such as `?via=`, then keep only the first path segment.
  const [first = ''] = target.split('?')[0].split('/');
  const match = ID_PATTERN.exec(first);
  if (!match) return null;
  if (match[1] === '@') return { kind: 'user', id: first };
  return { kind: 'room', id: first };
}
