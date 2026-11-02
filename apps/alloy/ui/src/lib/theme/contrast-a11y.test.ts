import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { flushSync, mount, unmount, type ComponentProps } from 'svelte';
import { describe, expect, it } from 'vitest';

import Avatar from '$lib/components/common/Avatar.svelte';
import {
  AVATAR_INK,
  AVATAR_WHITE,
  contrastRatio,
  parseCssColor,
  pickAvatarOnColor,
  relativeLuminance,
  type RgbColor
} from '$lib/identity/userColor';

/**
 * Wave-2 accessibility contrast guards (audit `docs/audits/a11y-2026-10.md`, A11Y-01/09/10/11/12/
 * 13/14/20; decision 0015).
 *
 * The Rust resolver is the source of truth and the theme generator refuses to report `passes_aa`
 * when one of these pairs regresses. This test independently measures the *generated* CSS the app
 * actually loads, and pins the UI-side derivations (`tokens.css`) to the same formulas, so a CSS or
 * component change cannot quietly reopen an audit finding.
 */

const read = (path: string) => readFileSync(resolve(process.cwd(), path), 'utf8');

const themesCss = read('src/lib/theme/generated/themes.css');
const tokensCss = read('src/lib/tokens.css');
const themesManifest = JSON.parse(read('src/lib/theme/generated/themes.json')) as Array<{
  id: string;
  mode: string;
}>;

/** Parse `[data-theme="id"] { --alloy-…: value; }` blocks from the generated theme CSS. */
function parseThemes(css: string): Map<string, Map<string, string>> {
  const themes = new Map<string, Map<string, string>>();
  const block = /\[data-theme="([^"]+)"\]\s*\{([\s\S]*?)\n\}/g;
  let match: RegExpExecArray | null;
  while ((match = block.exec(css)) !== null) {
    const vars = new Map<string, string>();
    for (const line of match[2].split('\n')) {
      const declaration = /^\s*(--alloy-[a-z0-9-]+)\s*:\s*(.+?);\s*$/.exec(line);
      if (declaration) vars.set(declaration[1], declaration[2].trim());
    }
    themes.set(match[1], vars);
  }
  return themes;
}

const THEMES = parseThemes(themesCss);
const THEME_IDS = [...THEMES.keys()].sort();
const MODES = new Map(themesManifest.map((entry) => [entry.id, entry.mode]));

function token(theme: string, name: string): string {
  const value = THEMES.get(theme)?.get(`--alloy-color-${name}`);
  if (value === undefined) throw new Error(`missing --alloy-color-${name} in theme ${theme}`);
  return value;
}

function color(theme: string, name: string): RgbColor {
  const parsed = parseCssColor(token(theme, name));
  if (!parsed) throw new Error(`unparseable --alloy-color-${name} '${token(theme, name)}'`);
  return parsed;
}

/** Contrast ratio of two theme tokens (`text.muted` -> `--alloy-color-text-muted`). */
function ratio(theme: string, foreground: string, background: string): number {
  return contrastRatio(color(theme, foreground), color(theme, background));
}

/** `color-mix(in srgb, fg <alpha>, bg)` as it composits when painted over an opaque background. */
function mix(fg: RgbColor, bg: RgbColor, alpha: number): RgbColor {
  return {
    r: fg.r * alpha + bg.r * (1 - alpha),
    g: fg.g * alpha + bg.g * (1 - alpha),
    b: fg.b * alpha + bg.b * (1 - alpha)
  };
}

function toHex(value: RgbColor): string {
  return `#${[value.r, value.g, value.b]
    .map((channel) => Math.round(channel).toString(16).padStart(2, '0'))
    .join('')}`;
}

/** Fail with the measured ratios, not just a boolean. */
function expectAllPass(failures: string[]): void {
  expect(failures, `pairs below 4.5:1:\n${failures.join('\n')}`).toEqual([]);
}

describe('theme palettes are present', () => {
  it('measures all four compiled themes', () => {
    expect(THEME_IDS).toEqual(['alloy-amoled', 'alloy-dark', 'alloy-light', 'midnight-ocean']);
    for (const theme of THEME_IDS) {
      for (let index = 1; index <= 12; index += 1) {
        expect(parseCssColor(token(theme, `user-${index}`))).not.toBeNull();
      }
    }
  });
});

describe('A11Y-01: avatar initials clear 4.5:1 on every user palette entry', () => {
  it('fails before the fix: white initials on the light dark-theme pastels', () => {
    // Evidence from the audit: #fbbf24 = 1.67:1, #4ade80 = 1.74:1, #22d3ee = 1.81:1.
    for (const fill of ['#fbbf24', '#4ade80', '#22d3ee']) {
      const parsed = parseCssColor(fill);
      expect(parsed).not.toBeNull();
      expect(contrastRatio(parseCssColor(AVATAR_WHITE) as RgbColor, parsed as RgbColor)).toBeLessThan(4.5);
    }
  });

  it('passes for all 12 palette entries in all four themes', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      for (let index = 1; index <= 12; index += 1) {
        const fill = token(theme, `user-${index}`);
        const chosen = pickAvatarOnColor(fill);
        const measured = contrastRatio(parseCssColor(chosen) as RgbColor, parseCssColor(fill) as RgbColor);
        if (measured < 4.5) failures.push(`${theme} user-${index} ${fill} on ${chosen}: ${measured.toFixed(3)}`);
      }
    }
    expectAllPass(failures);
  });

  it('chooses near-black on the light palettes and white on the dark one', () => {
    for (const theme of THEME_IDS) {
      const picks = new Set<string>();
      for (let index = 1; index <= 12; index += 1) picks.add(pickAvatarOnColor(token(theme, `user-${index}`)));
      if (theme === 'alloy-light') {
        expect([...picks]).toEqual([AVATAR_WHITE]);
      } else {
        expect([...picks]).toEqual([AVATAR_INK]);
      }
    }
  });

  it('falls back to white for unparseable fills instead of throwing', () => {
    expect(pickAvatarOnColor(null)).toBe(AVATAR_WHITE);
    expect(pickAvatarOnColor('')).toBe(AVATAR_WHITE);
    expect(pickAvatarOnColor('transparent')).toBe(AVATAR_WHITE);
    expect(pickAvatarOnColor('rgb(17, 17, 17)')).toBe(AVATAR_WHITE);
    expect(pickAvatarOnColor('#fbbf24')).toBe(AVATAR_INK);
  });

  it('matches the WCAG luminance of a known pair', () => {
    // #777777 on #ffffff measured 4.48:1 in the WCAG reference tools.
    const grey = parseCssColor('#777777') as RgbColor;
    const white = parseCssColor('#ffffff') as RgbColor;
    expect(relativeLuminance(white)).toBeCloseTo(1, 5);
    expect(relativeLuminance(parseCssColor('#000000') as RgbColor)).toBeCloseTo(0, 5);
    expect(contrastRatio(grey, white)).toBeCloseTo(4.478, 2);
  });
});

describe('A11Y-09: error text clears 4.5:1 on every surface, destructive fill keeps its label', () => {
  const surfaces = ['canvas', 'subtle', 'panel', 'elevated', 'hover'];

  it('error text passes on every surface in every theme', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      for (const surface of surfaces) {
        const measured = ratio(theme, 'status-error', `surface-${surface}`);
        if (measured < 4.5) failures.push(`${theme} error on ${surface}: ${measured.toFixed(3)}`);
      }
    }
    expectAllPass(failures);
  });

  it('filled error controls use the derived danger fill, not the text-tuned status.error', () => {
    for (const path of [
      'src/lib/components/common/Button.svelte',
      'src/lib/components/timeline/LiveLocationBanner.svelte',
      'src/lib/components/timeline/Timeline.svelte'
    ]) {
      expect(read(path), `${path} must paint filled destructive controls with --alloy-danger-fill`).toContain(
        'var(--alloy-danger-fill'
      );
    }
  });

  it('the destructive button keeps a 4.5:1 label on the derived danger fill', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      // tokens.css: color-mix(in srgb, status.error 70%, black); mirrored by the theme checker.
      const danger = mix(color(theme, 'status-error'), { r: 0, g: 0, b: 0 }, 0.7);
      const measured = contrastRatio(color(theme, 'text-on-accent'), danger);
      if (measured < 4.5) failures.push(`${theme} danger label on ${toHex(danger)}: ${measured.toFixed(3)}`);
    }
    expectAllPass(failures);
    expect(tokensCss).toMatch(
      /--alloy-danger-fill:\s*color-mix\(in srgb, var\(--alloy-color-status-error\) 70%, black\)/
    );
  });
});

describe('A11Y-10: midnight-ocean muted text clears 4.5:1 on every surface', () => {
  const surfaces = ['canvas', 'subtle', 'panel', 'elevated', 'hover', 'bubble-other'];

  it('muted text passes in every theme, on every surface', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      for (const surface of surfaces) {
        const background = surface === 'bubble-other' ? 'bubble-other-background' : `surface-${surface}`;
        const measured = ratio(theme, 'text-muted', background);
        if (measured < 4.5) failures.push(`${theme} muted on ${surface}: ${measured.toFixed(3)}`);
      }
    }
    expectAllPass(failures);
  });

  it('the audited midnight-ocean value is gone and would still fail', () => {
    const old = parseCssColor('#64748b') as RgbColor;
    const canvas = color('midnight-ocean', 'surface-canvas');
    expect(contrastRatio(old, canvas)).toBeLessThan(4.5);
    expect(token('midnight-ocean', 'text-muted').toLowerCase()).not.toBe('#64748b');
  });
});

describe('A11Y-11: the unread badge clears 4.5:1 in every theme', () => {
  it('on-accent text passes on the mention fill, which backs the unread badge', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      const measured = ratio(theme, 'text-on-accent', 'status-mention');
      if (measured < 4.5) failures.push(`${theme} badge text on mention: ${measured.toFixed(3)}`);
    }
    expectAllPass(failures);
  });

  it('tokens.css keeps one unread-badge source and the tab bar uses it', () => {
    expect(tokensCss).toMatch(/--alloy-unread-badge:\s*var\(--alloy-color-status-mention\)/);
    const tabBar = read('src/lib/components/shell/MobileTabBar.svelte');
    expect(tabBar).toContain('background: var(--alloy-unread-badge);');
    expect(tabBar).not.toContain('background: var(--alloy-color-status-mention);');
  });
});

describe('A11Y-12: link text clears 4.5:1 on every surface', () => {
  it('link passes on canvas, subtle, panel, elevated and hover', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      for (const surface of ['canvas', 'subtle', 'panel', 'elevated', 'hover']) {
        const measured = ratio(theme, 'text-link', `surface-${surface}`);
        if (measured < 4.5) failures.push(`${theme} link on ${surface}: ${measured.toFixed(3)}`);
      }
    }
    expectAllPass(failures);
  });

  it('the audited light link value is gone and would still fail on hover', () => {
    const old = parseCssColor('#2563eb') as RgbColor;
    expect(contrastRatio(old, color('alloy-light', 'surface-hover'))).toBeLessThan(4.5);
    expect(token('alloy-light', 'text-link').toLowerCase()).not.toBe('#2563eb');
  });
});

describe('A11Y-13: the derived own-bubble meta clears 4.5:1', () => {
  it('82% of the own text over the own fill passes in every theme', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      const background = color(theme, 'bubble-own-background');
      const meta = mix(color(theme, 'bubble-own-text'), background, 0.82);
      const measured = contrastRatio(meta, background);
      if (measured < 4.5) failures.push(`${theme} own meta ${toHex(meta)}: ${measured.toFixed(3)}`);
    }
    expectAllPass(failures);
    expect(tokensCss).toMatch(/--alloy-bubble-own-meta:[\s\S]*?var\(--alloy-color-bubble-own-text\) 82%/);
  });

  it('the audited midnight-ocean pair is gone and would still fail', () => {
    const oldText = parseCssColor('#d1e7f2') as RgbColor;
    const oldFill = parseCssColor('#0077b6') as RgbColor;
    expect(contrastRatio(oldText, oldFill)).toBeLessThan(4.5);
    expect(token('midnight-ocean', 'bubble-own-background').toLowerCase()).not.toBe('#0077b6');
  });
});

describe('A11Y-14: the translucent material worst case still reads', () => {
  // The checker models the pane fill over the strongest aurora bloom (accent-primary radial at its
  // peak alpha). Constants mirror the material recipe in tokens.css / ADR 0014.
  const RECIPE: Record<string, { chrome: number; chat: number; bloom: number }> = {
    light: { chrome: 0.86, chat: 0.92, bloom: 0.82 },
    dark: { chrome: 0.8, chat: 0.84, bloom: 0.42 }
  };

  function worstCase(theme: string): { chrome: RgbColor; chat: RgbColor } {
    // AMOLED inherits the dark material recipe.
    const recipe = RECIPE[MODES.get(theme) === 'light' ? 'light' : 'dark'];
    const canvas = color(theme, 'surface-canvas');
    const bloom = mix(color(theme, 'accent-primary'), canvas, recipe.bloom);
    return {
      chrome: mix(color(theme, 'surface-panel'), bloom, recipe.chrome),
      chat: mix(canvas, bloom, recipe.chat)
    };
  }

  it('primary, secondary, muted and link text pass on chrome and conversation worst cases', () => {
    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      const surfaces = worstCase(theme);
      for (const [surfaceName, surface] of Object.entries(surfaces)) {
        for (const [label, foreground] of [
          ['primary', 'text-primary'],
          ['secondary', 'text-secondary'],
          ['muted', 'text-muted'],
          ['link', 'text-link']
        ] as const) {
          const measured = contrastRatio(color(theme, foreground), surface);
          if (measured < 4.5) {
            failures.push(`${theme} ${label} on ${surfaceName} worst ${toHex(surface)}: ${measured.toFixed(3)}`);
          }
        }
      }
    }
    expectAllPass(failures);
  });

  it('floating glass over the scrim and over the settings backdrop also reads', () => {
    // Modal: glass-fill-strong (84% panel) over the 70% scrim over the brightest bloom.
    // Settings: glass-fill-strong over the 55% canvas scrim over the brightest bloom.
    const glassFill = /--alloy-glass-fill-strong:\s*color-mix\(\s*in srgb,\s*var\(--alloy-color-surface-panel\)\s*([\d.]+)%/
      .exec(tokensCss)?.[1];
    expect(glassFill, 'tokens.css defines the strong glass fill percentage').toBeDefined();
    const glassAlpha = Number(glassFill) / 100;

    const scrimRaw = token('alloy-light', 'scrim');
    const scrimAlpha = scrimRaw.length === 9 ? Number.parseInt(scrimRaw.slice(7, 9), 16) / 255 : 1;
    expect(scrimAlpha).toBeGreaterThan(0.5);

    const failures: string[] = [];
    for (const theme of THEME_IDS) {
      // AMOLED inherits the dark material recipe.
    const recipe = RECIPE[MODES.get(theme) === 'light' ? 'light' : 'dark'];
      const canvas = color(theme, 'surface-canvas');
      const bloom = mix(color(theme, 'accent-primary'), canvas, recipe.bloom);
      const scrim = mix(parseCssColor('#05070a') as RgbColor, bloom, scrimAlpha);
      const modalGlass = mix(color(theme, 'surface-panel'), scrim, glassAlpha);
      const settingsBackdrop = mix(canvas, bloom, 0.55);
      const settingsGlass = mix(color(theme, 'surface-panel'), settingsBackdrop, glassAlpha);
      for (const [surfaceName, surface] of [
        ['modal glass', modalGlass],
        ['settings glass', settingsGlass]
      ] as const) {
        for (const [label, foreground] of [
          ['primary', 'text-primary'],
          ['secondary', 'text-secondary'],
          ['muted', 'text-muted'],
          ['link', 'text-link'],
          ['error', 'status-error']
        ] as const) {
          const measured = contrastRatio(color(theme, foreground), surface);
          if (measured < 4.5) {
            failures.push(`${theme} ${label} on ${surfaceName}: ${measured.toFixed(3)}`);
          }
        }
      }
    }
    expectAllPass(failures);
  });
});

describe('A11Y-20: avatar presence is in the name and announced once', () => {
  function renderAvatar(props: ComponentProps<typeof Avatar>) {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const component = mount(Avatar, { target, props });
    flushSync();
    return { target, component };
  }

  it('puts presence in the accessible name and keeps the inner image decorative', () => {
    const { target, component } = renderAvatar({
      userId: '@mai:example.org',
      displayName: 'Mai',
      presence: 'online',
      src: 'https://example.org/avatar.png'
    });

    const root = target.querySelector('[role="img"]');
    expect(root).not.toBeNull();
    const label = root?.getAttribute('aria-label') ?? '';
    expect(label).toContain('Mai');
    expect(label).toContain('online');
    // The name is announced once, not repeated by a child alt text.
    expect(label.match(/Mai/g)).toHaveLength(1);
    expect(target.querySelector('img')?.getAttribute('alt')).toBe('');
    expect(target.querySelector('[role="img"] img')?.getAttribute('aria-label')).toBeNull();

    const dot = target.querySelector('[data-presence="online"]');
    expect(dot).not.toBeNull();
    expect(dot?.getAttribute('aria-hidden')).toBe('true');
    expect(dot?.getAttribute('aria-label')).toBeNull();

    unmount(component);
    target.remove();
  });

  it('has no presence suffix when no presence is rendered', () => {
    const { target, component } = renderAvatar({ displayName: 'Mai' });
    expect(target.querySelector('[role="img"]')?.getAttribute('aria-label')).toBe('Mai');
    expect(target.querySelector('[data-presence]')).toBeNull();
    unmount(component);
    target.remove();
  });

  it('keeps the initials colour resolved from the rendered fill', () => {
    const source = read('src/lib/components/common/Avatar.svelte');
    expect(source).toContain('pickAvatarOnColor(resolveAvatarFill(rootEl))');
    expect(source).toContain('void themeStore.activeTheme;');
    expect(source).not.toContain('color: var(--alloy-color-text-on-accent);');
  });
});
