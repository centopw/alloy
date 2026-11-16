/** Pure helpers for the pairing panels. Kept out of the components so they are testable. */

/** Unambiguous alphabet: no 0/O, 1/I/L, so a code read aloud cannot be mistyped into another. */
export const PAIRING_CODE_ALPHABET = '23456789ABCDEFGHJKMNPQRSTUVWXYZ';

export const PAIRING_CODE_LENGTH = 8;

/**
 * Normalises a typed or pasted pairing code: upper-case, then drop the spaces, dashes and other
 * separators a paste from a chat window carries. Anything past the 8th character is discarded so
 * the field can never hold more than one code.
 */
export function normalizePairingCode(input: string): string {
  return input
    .toUpperCase()
    .replace(/[^0-9A-Z]/g, '')
    .slice(0, PAIRING_CODE_LENGTH);
}

/** True when the normalised value is a complete code drawn from the pairing alphabet. */
export function isCompletePairingCode(input: string): boolean {
  const code = normalizePairingCode(input);
  if (code.length !== PAIRING_CODE_LENGTH) return false;
  return [...code].every((character) => PAIRING_CODE_ALPHABET.includes(character));
}

/**
 * Splits a 24-word recovery phrase. Commas and line breaks are accepted because phones like to
 * insert them; case is folded because the phrase is displayed lower-case and typed by hand.
 */
export function parseRecoveryWords(input: string): string[] {
  return input
    .trim()
    .toLowerCase()
    .split(/[\s,]+/)
    .filter((word) => word.length > 0);
}
