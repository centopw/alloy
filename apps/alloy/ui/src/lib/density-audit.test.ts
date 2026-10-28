import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * Guard for the Appearance › Density setting: every option must change what the user sees.
 *
 * The settings screen offers Compact, Normal and Cozy. The values live in `tokens.css` under
 * `[data-density]`; the message row is the component that spends them. A token that no component
 * reads makes the setting a placebo, which this test catches.
 */

// `?raw` returns nothing for CSS here, and jsdom module URLs are not file URLs.
const tokens = readFileSync(resolve(process.cwd(), 'src/lib/tokens.css'), 'utf8');
const rowSource = readFileSync(
  resolve(process.cwd(), 'src/lib/components/timeline/MessageRow.svelte'),
  'utf8'
);
const DENSITIES = ['compact', 'normal', 'cozy'] as const;

/** The declarations of one `[data-density="…"]` scope. */
function densityBlock(density: string): string {
  const start = tokens.indexOf(`[data-density="${density}"]`);
  expect(start, `${density} has a density scope`).toBeGreaterThanOrEqual(0);
  return tokens.slice(start, tokens.indexOf('}', start));
}

describe('density setting', () => {
  it('scales message padding across the three options', () => {
    const scales = DENSITIES.map((density) => {
      const match = /--alloy-spacing-message-padding:\s*var\(--alloy-spacing-scale-(\d)\)/.exec(
        densityBlock(density)
      );
      expect(match, `${density} sets message padding from the spacing scale`).not.toBeNull();
      return Number(match![1]);
    });
    expect(scales[0]).toBeLessThan(scales[1]);
    expect(scales[1]).toBeLessThan(scales[2]);
  });

  it('scales the avatar across the three options', () => {
    const sizes = DENSITIES.map((density) => {
      const match = /--alloy-spacing-avatar-size:\s*(\d+)px/.exec(densityBlock(density));
      expect(match, `${density} sets an avatar size`).not.toBeNull();
      return Number(match![1]);
    });
    expect(sizes[0]).toBeLessThan(sizes[1]);
    expect(sizes[1]).toBeLessThan(sizes[2]);
  });

  it('is spent by the message row, with the avatar sizes the scopes declare', () => {
    expect(rowSource).toContain('var(--alloy-spacing-message-padding)');
    for (const density of DENSITIES) {
      const px = /--alloy-spacing-avatar-size:\s*(\d+)px/.exec(densityBlock(density))![1];
      expect(rowSource, `the message row uses the ${density} avatar size`).toContain(
        `${density}: ${px}`
      );
    }
  });
});
