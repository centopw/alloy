const VALID_COOKIE_NAME = /^[\w.-]+$/;

function addCookie(cookies: Record<string, string>, name: string, value: string): void {
  const trimmedName = name.trim();
  const trimmedValue = value.trim();
  if (
    trimmedName === '__proto__' ||
    !VALID_COOKIE_NAME.test(trimmedName) ||
    trimmedValue.length === 0
  ) {
    return;
  }
  cookies[trimmedName] = trimmedValue;
}

function cookiesFromJson(text: string): Record<string, string> | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith('[') && !trimmed.startsWith('{')) return null;

  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return null;
  }

  const cookies: Record<string, string> = {};
  if (Array.isArray(parsed)) {
    for (const item of parsed) {
      if (typeof item !== 'object' || item === null) continue;
      const entry = item as Record<string, unknown>;
      if (typeof entry.name === 'string' && typeof entry.value === 'string') {
        addCookie(cookies, entry.name, entry.value);
      }
    }
    return cookies;
  }

  if (typeof parsed === 'object' && parsed !== null) {
    for (const [name, value] of Object.entries(parsed)) {
      if (typeof value === 'string') {
        addCookie(cookies, name, value);
      }
    }
    return cookies;
  }

  return null;
}

/**
 * Pulls the cookies out of what a user copied from a browser's Network tab or cookie extension:
 * a JSON export (Cookie-Editor / EditThisCookie array of `{ name, value }` or a plain
 * `{ [name]: value }` object), a request copied as cURL (bash or Windows cmd flavour), a bare
 * `Cookie:` header line, or just the header's value. Returns name → value for every cookie found;
 * an empty object when the text carries none.
 */
export function cookiesFromPaste(text: string): Record<string, string> {
  const fromJson = cookiesFromJson(text);
  if (fromJson !== null) return fromJson;

  // cmd-flavoured cURL escapes quotes and percent signs with a caret (`^"`, `^%`).
  const source = text.includes('^"') ? text.replace(/\^([\s\S])/g, '$1') : text;
  const header =
    /(?:-H|--header)\s+\$?(['"])cookie:\s*([\s\S]*?)\1/i.exec(source)?.[2] ??
    /(?:-b|--cookie)\s+\$?(['"])([\s\S]*?)\1/.exec(source)?.[2] ??
    /^\s*cookie:\s*(.*)$/im.exec(source)?.[1] ??
    // Anything else that still looks like a command is a request without cookies, not a header.
    (/^\s*curl\s/i.test(source) ? '' : source);

  const cookies: Record<string, string> = {};
  for (const pair of header.split(';')) {
    const eq = pair.indexOf('=');
    if (eq <= 0) continue;
    addCookie(cookies, pair.slice(0, eq), pair.slice(eq + 1));
  }
  return cookies;
}
