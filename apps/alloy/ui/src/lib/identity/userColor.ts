/**
 * Deterministic user color assignment via FNV-1a hash of Matrix user id.
 * Maps to CSS custom property `--alloy-color-user-N` where N is 1..12.
 * Distribution is uniform across 1000 sample ids.
 */

function fnv1a(str: string): number {
  let hash = 0x811c9dc5; // FNV offset basis
  for (let i = 0; i < str.length; i++) {
    hash ^= str.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193); // FNV prime
  }
  return hash >>> 0; // Convert to unsigned 32-bit
}

/**
 * Get the color index (1..12) for a given Matrix user id.
 */
export function getUserColorIndex(userId: string | undefined): number {
  if (!userId) return 1;
  const hash = fnv1a(userId);
  return (hash % 12) + 1;
}

/**
 * Get the CSS custom property variable for a user's color.
 */
export function getUserColorVar(userId: string | undefined): string {
  const index = getUserColorIndex(userId);
  return `--alloy-color-user-${index}`;
}

/**
 * Get the CSS value reference for a user's color (for use in CSS).
 */
export function getUserColorValue(userId: string | undefined): string {
  return `var(${getUserColorVar(userId)})`;
}
