import {
  createCatalog,
  direction,
  formatMessage,
  isLocale,
  LOCALE_STORAGE_KEY,
  resolveLocale,
  type Locale,
  type MessageArgs,
} from './core';
import en from './en.ftl?raw';
import vi from './vi.ftl?raw';

const catalog = createCatalog({ en, vi });

/** `?locale=` is a QA/verification override (used for the RTL pseudo-locale), dev builds only. */
function queryLocale(): string | null {
  if (!import.meta.env.DEV || typeof window === 'undefined') return null;
  return new URLSearchParams(window.location.search).get('locale');
}

function readStoredLocale(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    return window.localStorage.getItem(LOCALE_STORAGE_KEY);
  } catch {
    return null;
  }
}

function browserLanguages(): readonly string[] {
  if (typeof navigator === 'undefined') return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

function initialLocale(): Locale {
  if (typeof window === 'undefined') return 'en';
  const override = queryLocale();
  if (override && isLocale(override)) return override;
  return resolveLocale({ stored: readStoredLocale(), languages: browserLanguages() });
}

const state = $state<{ locale: Locale }>({ locale: initialLocale() });

/**
 * Applies the locale to the document. `dir` and `lang` are read by CSS (`[dir='rtl']`) and by
 * assistive tech, and are the only place the locale touches the DOM directly.
 */
function applyToDocument(locale: Locale) {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  root.lang = locale === 'rtl' ? 'en' : locale;
  root.dir = direction(locale);
  root.dataset.locale = locale;
}

applyToDocument(state.locale);

/** Current locale. Reading it inside a template makes the template re-render on a switch. */
export function currentLocale(): Locale {
  return state.locale;
}

export function localeDirection(): 'ltr' | 'rtl' {
  return direction(state.locale);
}

/**
 * Switches the locale live: every message already on screen re-renders because `t()` reads the
 * reactive locale. Strings a store captured earlier (an error toast, a completed operation's
 * message) keep the language they were created in until the next event.
 */
export function setLocale(locale: Locale) {
  state.locale = locale;
  applyToDocument(locale);
  try {
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // Storage can be unavailable (private mode, embedded webview); the switch still applies.
  }
}

/** Formats a message id for the current locale. */
export function t(id: string, args?: MessageArgs): string {
  return formatMessage(catalog(state.locale), state.locale, id, args);
}

/** Exposed for tests and for scripts that pre-format a batch outside a component. */
export function formatFor(locale: Locale, id: string, args?: MessageArgs): string {
  return formatMessage(catalog(locale), locale, id, args);
}
