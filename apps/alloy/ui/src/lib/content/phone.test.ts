import { describe, expect, it } from 'vitest';
import { COUNTRIES, defaultCountry, parsePhone } from './phone.js';

describe('parsePhone', () => {
  it('reads every way of typing the same Vietnamese number', () => {
    for (const typed of [
      '+84 367337123',
      '+84 0367337123',
      '+84367337123',
      '0084 367 337 123',
      '0367337123',
      '367337123',
      '84367337123',
      '(036) 733-7123',
    ]) {
      expect(parsePhone(typed, '84').e164, typed).toBe('+84367337123');
    }
  });

  it('lets a typed country code override the selected one', () => {
    expect(parsePhone('+1 (415) 555-0100', '84')).toEqual({ dial: '1', e164: '+14155550100' });
    expect(parsePhone('+44 07911 123456', '84')).toEqual({ dial: '44', e164: '+447911123456' });
  });

  it('keeps a national number that only happens to start with the calling code', () => {
    expect(parsePhone('0841234567', '84').e164).toBe('+84841234567');
  });

  it('keeps the leading zero Italian numbers carry', () => {
    expect(parsePhone('06 1234 5678', '39').e164).toBe('+390612345678');
  });

  it('returns nothing for input without digits', () => {
    expect(parsePhone('  +', '84').e164).toBe('');
  });
});

describe('countries', () => {
  it('lists each region once with a numeric calling code', () => {
    expect(new Set(COUNTRIES.map((c) => c.iso)).size).toBe(COUNTRIES.length);
    for (const c of COUNTRIES) expect(c.dial, c.iso).toMatch(/^[1-9]\d{0,2}$/);
  });

  it('preselects the region of the device locale', () => {
    expect(defaultCountry(['vi-VN', 'en-US'])).toBe('VN');
    expect(defaultCountry(['en', 'fr-FR'])).toBe('FR');
    expect(defaultCountry(['en'])).toBe('US');
  });
});
