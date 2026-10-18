/**
 * Platform-aware keyboard shortcut labels. The primary modifier is Cmd on Apple platforms and
 * Ctrl everywhere else, so hints shown to the user match the key they actually press.
 */

export type ModKey = 'Ctrl' | 'Cmd';

/** Platform string used for detection: `userAgentData.platform`, falling back to `navigator.platform`. */
export function currentPlatform(): string {
  if (typeof navigator === 'undefined') return '';
  const uad = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
  return uad?.platform || navigator.platform || '';
}

/** Primary modifier for `platform` (defaults to the running platform). */
export function modKey(platform: string = currentPlatform()): ModKey {
  return /mac|iphone|ipad|ipod|darwin/i.test(platform) ? 'Cmd' : 'Ctrl';
}

/** Human label for a modifier-plus-key shortcut, e.g. `shortcutLabel('K')` => `Cmd K` on macOS. */
export function shortcutLabel(key: string, platform?: string): string {
  return `${modKey(platform)} ${key}`;
}
