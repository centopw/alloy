import { currentLocale } from '$lib/i18n';

/**
 * Formats an engine timestamp for display. `null` means "never" and is left to the caller, which
 * owns the localized word for it.
 */
export function formatSyncTime(ms: number | null): string | null {
  if (ms === null) return null;
  const locale = currentLocale();
  return new Intl.DateTimeFormat(locale === 'rtl' ? 'en' : locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(ms));
}

/** Groups a hex fingerprint into 4-character blocks so it can be compared by eye. */
export function groupPin(pin: string): string {
  const compact = pin.replace(/\s+/g, '').toUpperCase();
  return compact.replace(/(.{4})/g, '$1 ').trim();
}
