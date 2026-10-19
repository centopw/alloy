import { describe, expect, it } from 'vitest';
import { placeFlyout } from './contextmenu';

const VIEWPORT = { width: 1000, height: 800 };

describe('placeFlyout', () => {
  it('opens to the right of the parent row, aligned to its top edge', () => {
    const row = { x: 100, y: 50, width: 200, height: 32 };
    expect(placeFlyout(row, { width: 150, height: 200 }, VIEWPORT)).toEqual({ x: 308, y: 50 });
  });

  it('flips to the left of the parent row when the right side overflows', () => {
    const row = { x: 800, y: 50, width: 150, height: 32 };
    expect(placeFlyout(row, { width: 200, height: 100 }, VIEWPORT)).toEqual({ x: 592, y: 50 });
  });

  it('clamps to the left margin when neither side fits', () => {
    const row = { x: 20, y: 50, width: 20, height: 32 };
    expect(placeFlyout(row, { width: 980, height: 100 }, VIEWPORT)).toEqual({ x: 8, y: 50 });
  });

  it('clamps the flyout above the bottom margin when the row is near the bottom', () => {
    const row = { x: 100, y: 780, width: 200, height: 32 };
    expect(placeFlyout(row, { width: 150, height: 200 }, VIEWPORT)).toEqual({ x: 308, y: 592 });
  });

  it('clamps the flyout below the top margin when the row is above the viewport', () => {
    const row = { x: 100, y: -50, width: 200, height: 32 };
    expect(placeFlyout(row, { width: 150, height: 200 }, VIEWPORT)).toEqual({ x: 308, y: 8 });
  });
});
