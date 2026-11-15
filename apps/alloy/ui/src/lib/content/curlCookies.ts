/**
 * Pulls the cookies out of what a user copied from a browser's Network tab: a request copied as
 * cURL (bash or Windows cmd flavour), a bare `Cookie:` header line, or just the header's value.
 * Returns name → value for every cookie found; an empty object when the text carries none.
 */
export function cookiesFromPaste(text: string): Record<string, string> {
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
    const name = pair.slice(0, eq).trim();
    const value = pair.slice(eq + 1).trim();
    if (/^[\w.-]+$/.test(name) && value.length > 0) cookies[name] = value;
  }
  return cookies;
}
