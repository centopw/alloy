import { currentLocale, t } from '$lib/i18n';

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

/** One user-facing name per store layer; an unknown key is shown as the engine names it. */
export function layerLabel(key: string): string {
  switch (key) {
    case 'settings':
      return t('sync-layer-settings');
    case 'tags':
      return t('sync-layer-tags');
    case 'catalogue':
      return t('sync-layer-catalogue');
    case 'backgrounds':
      return t('sync-layer-backgrounds');
    case 'theme':
      return t('sync-layer-theme');
    case 'messages':
      return t('sync-layer-messages');
    default:
      return key;
  }
}
