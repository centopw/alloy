/**
 * Public entry point for UI copy. Components import `t` from here; the reactive state lives in
 * `i18n.svelte.ts` because Svelte only compiles runes inside `.svelte.ts` modules.
 */
export {
  currentLocale,
  formatFor,
  localeDirection,
  setLocale,
  t,
} from './i18n.svelte.js';
export {
  DEFAULT_LOCALE,
  LOCALE_STORAGE_KEY,
  RTL_PSEUDO_LOCALE,
  SHIPPED_LOCALES,
  direction,
  isLocale,
  resolveLocale,
  type Locale,
  type MessageArgs,
  type ShippedLocale,
} from './core.js';
