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

// ---------------------------------------------------------------------------
// Initials contrast (A11Y-01)
// ---------------------------------------------------------------------------

/**
 * The two colours initials can be painted in on top of a user-colour fill.
 *
 * The dark, AMOLED and midnight palettes are light pastels, so the old fixed white initials fell
 * to 1.67-2.98:1 (WCAG AA for text is 4.5:1). Picking between white and this near-black from the
 * fill's luminance guarantees the chosen colour clears 4.5:1 for any possible fill: the worst
 * case is the crossover luminance, where both candidates measure ~4.51:1.
 */
export const AVATAR_WHITE = '#ffffff';
export const AVATAR_INK = '#050505';

export interface RgbColor {
  r: number;
  g: number;
  b: number;
}

/**
 * Parse the colour formats a theme token can resolve to in `getComputedStyle`: `#rgb`, `#rgba`,
 * `#rrggbb`, `#rrggbbaa`, `rgb(r, g, b)` and `rgba(r, g, b, a)`. Alpha is dropped (avatar fills are
 * opaque and the initials sit directly on the fill).
 */
export function parseCssColor(value: string | null | undefined): RgbColor | null {
  if (!value) return null;
  const trimmed = value.trim();

  if (trimmed.startsWith('#')) {
    const hex = trimmed.slice(1);
    let digits: string;
    if (hex.length === 3 || hex.length === 4) {
      digits = [...hex.slice(0, 3)].map((c) => c + c).join('');
    } else if (hex.length === 6 || hex.length === 8) {
      digits = hex.slice(0, 6);
    } else {
      return null;
    }
    if (!/^[0-9a-f]{6}$/i.test(digits)) return null;
    return {
      r: Number.parseInt(digits.slice(0, 2), 16),
      g: Number.parseInt(digits.slice(2, 4), 16),
      b: Number.parseInt(digits.slice(4, 6), 16)
    };
  }

  const fn = /^rgba?\(\s*([\d.]+)\s*[, ]\s*([\d.]+)\s*[, ]\s*([\d.]+)/i.exec(trimmed);
  if (!fn) return null;
  const clamp = (n: number) => Math.min(255, Math.max(0, n));
  return { r: clamp(Number(fn[1])), g: clamp(Number(fn[2])), b: clamp(Number(fn[3])) };
}

/**
 * WCAG 2.1 relative luminance of an sRGB colour (0 = black, 1 = white).
 */
export function relativeLuminance(color: RgbColor): number {
  const channel = (c: number) => {
    const value = c / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
}

/**
 * WCAG 2.1 contrast ratio between two opaque sRGB colours (1..21).
 */
export function contrastRatio(a: RgbColor, b: RgbColor): number {
  const first = relativeLuminance(a);
  const second = relativeLuminance(b);
  const lighter = Math.max(first, second);
  const darker = Math.min(first, second);
  return (lighter + 0.05) / (darker + 0.05);
}

/**
 * Pick the readable initials colour for an avatar painted with `background`.
 *
 * `background` is the colour the component resolved from CSS (`getComputedStyle(...).backgroundColor`
 * or a custom property value); unparseable values fall back to white rather than throwing, because
 * a visual fallback is better than a broken avatar. The same rule is mirrored in Rust by
 * `ThemeResolver::avatar_initials_color` and checked for every theme palette by the theme checker.
 */
export function pickAvatarOnColor(
  background: string | RgbColor | null | undefined
): typeof AVATAR_WHITE | typeof AVATAR_INK {
  const fill = typeof background === 'string' ? parseCssColor(background) : (background ?? null);
  if (!fill) return AVATAR_WHITE;
  const white = parseCssColor(AVATAR_WHITE);
  const ink = parseCssColor(AVATAR_INK);
  if (!white || !ink) return AVATAR_WHITE;
  return contrastRatio(white, fill) >= contrastRatio(ink, fill) ? AVATAR_WHITE : AVATAR_INK;
}
