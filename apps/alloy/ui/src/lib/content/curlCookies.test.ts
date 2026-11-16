import { describe, expect, it } from 'vitest';
import { cookiesFromPaste } from './curlCookies.js';

const expected = { datr: 'abc-DEF_1', c_user: '100001234', xs: '12%3Atoken%3A2%3A1700000000' };

describe('cookiesFromPaste', () => {
  it('reads the cookie header of a bash cURL command', () => {
    const paste = [
      "curl 'https://www.messenger.com/api/graphql/' \\",
      "  -H 'accept: */*' \\",
      "  -H 'cookie: datr=abc-DEF_1; c_user=100001234; xs=12%3Atoken%3A2%3A1700000000' \\",
      "  -H 'user-agent: Mozilla/5.0' \\",
      "  --data-raw 'fb_dtsg=x&a=b; c=d'",
    ].join('\n');
    expect(cookiesFromPaste(paste)).toEqual(expected);
  });

  it('reads the -b form newer browsers copy', () => {
    const paste =
      "curl 'https://www.messenger.com/' -H 'accept: */*' -b 'datr=abc-DEF_1; c_user=100001234; xs=12%3Atoken%3A2%3A1700000000'";
    expect(cookiesFromPaste(paste)).toEqual(expected);
  });

  it('reads a Windows cmd cURL command with caret escapes', () => {
    const paste =
      'curl ^"https://www.messenger.com/^" ^\n  -H ^"accept: */*^" ^\n  -b ^"datr=abc-DEF_1; c_user=100001234; xs=12^%3Atoken^%3A2^%3A1700000000^"';
    expect(cookiesFromPaste(paste)).toEqual(expected);
  });

  it('reads a bare Cookie header line and a bare header value', () => {
    const value = 'datr=abc-DEF_1; c_user=100001234; xs=12%3Atoken%3A2%3A1700000000';
    expect(cookiesFromPaste(`Cookie: ${value}`)).toEqual(expected);
    expect(cookiesFromPaste(value)).toEqual(expected);
  });

  it('keeps "=" inside a value', () => {
    expect(cookiesFromPaste('sb=a=b==; fr=1')).toEqual({ sb: 'a=b==', fr: '1' });
  });

  it('reads a Cookie-Editor / EditThisCookie JSON array export', () => {
    const paste = JSON.stringify([
      {
        domain: '.messenger.com',
        expirationDate: 1735689600,
        hostOnly: false,
        httpOnly: true,
        name: 'datr',
        path: '/',
        sameSite: 'no_restriction',
        secure: true,
        value: 'abc-DEF_1',
      },
      { name: 'c_user', value: '100001234' },
      { name: 'xs', value: '12%3Atoken%3A2%3A1700000000' },
      null,
      { name: 'empty', value: '' },
      { name: 123, value: 'ignored' },
    ]);
    expect(cookiesFromPaste(`  \n${paste}\n  `)).toEqual(expected);
  });

  it('reads a plain JSON object of cookie name to value', () => {
    const paste = JSON.stringify({
      datr: 'abc-DEF_1',
      c_user: '100001234',
      xs: '12%3Atoken%3A2%3A1700000000',
      ignoredNumber: 42,
      empty: '',
    });
    expect(cookiesFromPaste(paste)).toEqual(expected);
  });

  it('falls back when text starting with [ or { is not valid JSON', () => {
    expect(cookiesFromPaste('{not json; datr=abc-DEF_1; c_user=100001234; xs=12%3Atoken%3A2%3A1700000000')).toEqual(
      expected,
    );
    expect(cookiesFromPaste('[not valid json')).toEqual({});
    expect(cookiesFromPaste('[]')).toEqual({});
    expect(cookiesFromPaste('{}')).toEqual({});
  });

  it('finds nothing in a request without cookies or in prose', () => {
    expect(cookiesFromPaste("curl 'https://example.com/?a=b;c=d' -H 'accept: */*'")).toEqual({});
    expect(cookiesFromPaste('hello there')).toEqual({});
    expect(cookiesFromPaste('')).toEqual({});
  });
});
