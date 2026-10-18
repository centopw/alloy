/** Shown when a Matrix ID has no usable localpart (for example `@:example.org`). */
const UNKNOWN_NAME = 'Unknown user';

/**
 * Localpart of a Matrix user ID: the text between the leading `@` and the first `:`.
 * Tolerates a missing sigil (`alice:example.org`) and a missing domain (`@alice`).
 * Returns `''` when no localpart can be found.
 */
export function localpart(userId: string): string {
  let body = userId.trim();
  if (body.startsWith('@')) body = body.slice(1);
  const colon = body.indexOf(':');
  return (colon === -1 ? body : body.slice(0, colon)).trim();
}

/**
 * Name to show for a user, in priority order: the explicit display name, the member's
 * display name in the room, then the localpart. The full Matrix ID is never returned.
 */
export function displayNameFor(
  userId: string,
  explicit?: string | null,
  memberName?: string | null
): string {
  return explicit?.trim() || memberName?.trim() || localpart(userId) || UNKNOWN_NAME;
}
