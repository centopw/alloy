import { FluentBundle, FluentResource } from '@fluent/bundle';

/** Shipped locales. `en` is the source of truth; every bundle must carry the same message ids. */
export const SHIPPED_LOCALES = ['en', 'vi'] as const;
export type ShippedLocale = (typeof SHIPPED_LOCALES)[number];

/**
 * `rtl` is a verification pseudo-locale, not a shipped translation: it renders the English
 * messages wrapped in `⟦…⟧` with `dir=rtl`, so layout mirroring and text growth can be inspected
 * without translating a whole language first. It is selectable only through the explicit
 * `?locale=rtl` / stored preference path, never by browser-language detection.
 */
export const RTL_PSEUDO_LOCALE = 'rtl' as const;
export type Locale = ShippedLocale | typeof RTL_PSEUDO_LOCALE;

export const DEFAULT_LOCALE: ShippedLocale = 'en';
export const LOCALE_STORAGE_KEY = 'alloy.locale.v1';

/** Languages that ship a real translation. Anything else resolves to English. */
const BROWSER_LANGUAGE_MAP: ReadonlyArray<readonly [string, Locale]> = [
  ['vi', 'vi'],
  ['rtl', RTL_PSEUDO_LOCALE],
];

/**
 * Picks the startup locale: an explicit stored choice wins, then the browser language, then
 * English. The pseudo-locale is never inferred from a browser language.
 */
export function resolveLocale(input: {
  stored?: string | null;
  languages?: readonly string[];
}): Locale {
  const stored = input.stored?.trim().toLowerCase();
  if (stored && isLocale(stored) && stored !== RTL_PSEUDO_LOCALE) return stored;
  for (const language of input.languages ?? []) {
    const base = language.toLowerCase().split('-')[0];
    const match = BROWSER_LANGUAGE_MAP.find(([tag]) => tag === base);
    if (match) return match[1];
  }
  return DEFAULT_LOCALE;
}

export function isLocale(value: string): value is Locale {
  return value === RTL_PSEUDO_LOCALE || (SHIPPED_LOCALES as readonly string[]).includes(value);
}

export function direction(locale: Locale): 'ltr' | 'rtl' {
  return locale === RTL_PSEUDO_LOCALE ? 'rtl' : 'ltr';
}

export type MessageArgs = Record<string, string | number | null | undefined>;

export interface Bundles {
  /** Messages of the requested locale, missing ids are looked up in the fallback bundle. */
  bundle: FluentBundle;
  fallback: FluentBundle;
}

export class MessageCatalogError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'MessageCatalogError';
  }
}

/** Parses one FTL source. Every syntax error is fatal: a half-parsed bundle ships broken copy. */
export function parseBundle(locale: Locale, source: string): FluentBundle {
  const bundle = new FluentBundle(locale, {
    // Isolation marks would leak into screenshot diffs and `textContent` assertions; the app is
    // single-direction per document, so plain interpolation is enough here.
    useIsolating: false,
  });
  const resource = new FluentResource(source);
  const errors = bundle.addResource(resource);
  if (errors.length > 0) {
    throw new MessageCatalogError(
      `${locale}.ftl: ${errors.map((error) => error.message).join('; ')}`
    );
  }
  return bundle;
}

/**
 * Builds the formatter for one locale. The fallback bundle is English so a missing translation
 * degrades to real copy instead of a raw id.
 */
export function createCatalog(
  sources: Readonly<Record<ShippedLocale, string>>
): (locale: Locale) => Bundles {
  const english = parseBundle('en', sources.en);
  const cache = new Map<Locale, Bundles>();
  return (locale: Locale) => {
    const cached = cache.get(locale);
    if (cached) return cached;
    // The pseudo-locale has no catalog of its own: it reads English and wraps the result in
    // `formatMessage`, so it must not be looked up in the shipped sources.
    const bundle =
      locale === 'en' || locale === RTL_PSEUDO_LOCALE ? english : parseBundle(locale, sources[locale]);
    const bundles = { bundle, fallback: english };
    cache.set(locale, bundles);
    return bundles;
  };
}

const PSEUDO_OPEN = '⟦';
const PSEUDO_CLOSE = '⟧';

/**
 * Formats `id`, falling back to English, then to the id itself. Arguments never become part of
 * the fallback: a missing message is a bug the checker reports, not something to paper over.
 */
export function formatMessage(
  bundles: Bundles,
  locale: Locale,
  id: string,
  args?: MessageArgs
): string {
  const message = bundles.bundle.getMessage(id) ?? bundles.fallback.getMessage(id);
  if (!message?.value) return id;
  const values: Record<string, string | number> = {};
  for (const [key, value] of Object.entries(args ?? {})) {
    if (value !== null && value !== undefined) values[key] = value;
  }
  const errors: Error[] = [];
  const text = bundles.bundle.formatPattern(message.value, values, errors);
  if (text === '') return id;
  return locale === RTL_PSEUDO_LOCALE ? `${PSEUDO_OPEN}${text}${PSEUDO_CLOSE}` : text;
}

/** Message ids present in a bundle source. Used by the parity check and its unit test. */
export function messageIds(source: string): string[] {
  const resource = new FluentResource(source);
  return resource.body.map((entry) => entry.id);
}
