/**
 * Country calling codes and the clean-up that turns what people type into the international
 * format a network wants (`+84367337123`), whatever mix of `+84`, trunk `0` and spacing they used.
 */

// ISO 3166 region followed by its calling code. NANP members share `1`.
const TABLE =
  'AF93 AL355 DZ213 AS1 AD376 AO244 AI1 AG1 AR54 AM374 AW297 AU61 AT43 AZ994 BS1 BH973 BD880 ' +
  'BB1 BY375 BE32 BZ501 BJ229 BM1 BT975 BO591 BA387 BW267 BR55 VG1 BN673 BG359 BF226 BI257 ' +
  'KH855 CM237 CA1 CV238 KY1 CF236 TD235 CL56 CN86 CO57 KM269 CG242 CD243 CK682 CR506 CI225 ' +
  'HR385 CU53 CW599 CY357 CZ420 DK45 DJ253 DM1 DO1 EC593 EG20 SV503 GQ240 ER291 EE372 SZ268 ' +
  'ET251 FK500 FO298 FJ679 FI358 FR33 GF594 PF689 GA241 GM220 GE995 DE49 GH233 GI350 GR30 ' +
  'GL299 GD1 GP590 GU1 GT502 GN224 GW245 GY592 HT509 HN504 HK852 HU36 IS354 IN91 ID62 IR98 ' +
  'IQ964 IE353 IL972 IT39 JM1 JP81 JO962 KZ7 KE254 KI686 XK383 KW965 KG996 LA856 LV371 LB961 ' +
  'LS266 LR231 LY218 LI423 LT370 LU352 MO853 MG261 MW265 MY60 MV960 ML223 MT356 MH692 MQ596 ' +
  'MR222 MU230 MX52 FM691 MD373 MC377 MN976 ME382 MS1 MA212 MZ258 MM95 NA264 NR674 NP977 ' +
  'NL31 NC687 NZ64 NI505 NE227 NG234 NU683 KP850 MK389 MP1 NO47 OM968 PK92 PW680 PS970 PA507 ' +
  'PG675 PY595 PE51 PH63 PL48 PT351 PR1 QA974 RE262 RO40 RU7 RW250 KN1 LC1 VC1 WS685 SM378 ' +
  'ST239 SA966 SN221 RS381 SC248 SL232 SG65 SX1 SK421 SI386 SB677 SO252 ZA27 KR82 SS211 ES34 ' +
  'LK94 SD249 SR597 SE46 CH41 SY963 TW886 TJ992 TZ255 TH66 TL670 TG228 TO676 TT1 TN216 TR90 ' +
  'TM993 TC1 TV688 UG256 UA380 AE971 GB44 US1 UY598 UZ998 VU678 VA39 VE58 VN84 VI1 YE967 ' +
  'ZM260 ZW263';

export interface Country {
  /** ISO 3166-1 alpha-2 region code. */
  iso: string;
  /** Calling code digits, without the `+`. */
  dial: string;
}

export const COUNTRIES: Country[] = TABLE.split(' ').map((entry) => ({
  iso: entry.slice(0, 2),
  dial: entry.slice(2),
}));

const DIALS = new Set(COUNTRIES.map((country) => country.dial));

// Italy (and the Vatican on +39) keep the leading 0 as part of the number itself.
const KEEPS_LEADING_ZERO = new Set(['39']);

/** The calling code an international number starts with, or null when no country has it. */
function dialOf(digits: string): string | null {
  for (let length = 3; length >= 1; length--) {
    const candidate = digits.slice(0, length);
    if (DIALS.has(candidate)) return candidate;
  }
  return null;
}

/** The country to preselect: the region of the device's locale, else the United States. */
export function defaultCountry(locales: readonly string[]): string {
  for (const locale of locales) {
    const region = /[-_]([A-Za-z]{2})\b/.exec(locale)?.[1]?.toUpperCase();
    if (region && COUNTRIES.some((country) => country.iso === region)) return region;
  }
  return 'US';
}

export interface ParsedPhone {
  /** The calling code in effect: the one typed after `+`/`00`, else the selected country's. */
  dial: string;
  /** International format, `+` and digits only; empty when no digits were typed. */
  e164: string;
}

/**
 * Reads a phone number typed in any common shape. `selectedDial` is the dropdown's calling code;
 * a number typed with its own `+` or `00` prefix overrides it.
 *
 * `+84 367337123`, `+84 0367337123`, `0367337123`, `367337123` and `84367337123` all become
 * `+84367337123` with Vietnam selected.
 */
export function parsePhone(input: string, selectedDial: string): ParsedPhone {
  const trimmed = input.trim();
  let digits = trimmed.replace(/\D/g, '');
  let dial = selectedDial;

  const international = trimmed.startsWith('+') || digits.startsWith('00');
  if (international) {
    digits = digits.replace(/^00/, '');
    const typed = digits.startsWith(selectedDial) ? selectedDial : dialOf(digits);
    if (typed === null) return { dial, e164: digits ? `+${digits}` : '' };
    dial = typed;
    digits = digits.slice(dial.length);
  } else if (!digits.startsWith('0') && digits.startsWith(dial) && digits.length >= 11) {
    // The calling code typed without its `+`: a national number is never this long.
    digits = digits.slice(dial.length);
  }

  if (!KEEPS_LEADING_ZERO.has(dial)) digits = digits.replace(/^0+/, '');
  return { dial, e164: digits ? `+${dial}${digits}` : '' };
}
