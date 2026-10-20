import { describe, expect, it } from 'vitest';
import { placeDesktopReactionStrip, placeFlyout, placeReactionBar } from './contextmenu';

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

describe('placeReactionBar', () => {
  const phone = { width: 390, height: 780 };
  const limits = { top: 56, bottom: 560 };
  const bar = { width: 300, height: 52 };
  const rowAt = (y: number) => ({ x: 0, y, width: 390, height: 60 });

  it('floats centred above the pressed message', () => {
    expect(placeReactionBar(rowAt(400), bar, phone, limits)).toEqual({ x: 45, y: 340 });
  });

  it('drops below the message when there is no room above', () => {
    expect(placeReactionBar(rowAt(60), bar, phone, limits)).toEqual({ x: 45, y: 128 });
  });

  it('stays above the action sheet for a message near the bottom', () => {
    expect(placeReactionBar(rowAt(700), bar, phone, limits)).toEqual({ x: 45, y: 500 });
  });

  it('keeps a bar wider than the screen at the left margin', () => {
    expect(placeReactionBar(rowAt(400), { width: 500, height: 52 }, phone, limits).x).toBe(8);
  });

  it('aligns to the right edge when anchor.align is right', () => {
    // bubble at x=100, width=200 -> right edge is 300. Bar width 240 -> x = 300 - 240 = 60
    const bubble = { x: 100, y: 400, width: 200, height: 50, align: 'right' as const };
    expect(placeReactionBar(bubble, { width: 240, height: 52 }, phone, limits).x).toBe(60);
  });

  it('aligns to the left edge when anchor.align is left', () => {
    const bubble = { x: 50, y: 400, width: 200, height: 50, align: 'left' as const };
    expect(placeReactionBar(bubble, { width: 240, height: 52 }, phone, limits).x).toBe(50);
  });
});

describe('placeDesktopReactionStrip', () => {
  const viewport = { width: 1200, height: 800 };
  const card = { x: 400, y: 300 };
  const cardSize = { width: 220, height: 260 };
  const stripSize = { width: 260, height: 40 };

  it('places strip directly above the card with gap', () => {
    expect(placeDesktopReactionStrip(card, cardSize, stripSize, viewport, 8, 6)).toEqual({
      x: 400,
      y: 254, // 300 - 40 - 6
    });
  });

  it('clamps y to margin when card is near the top', () => {
    expect(placeDesktopReactionStrip({ x: 400, y: 30 }, cardSize, stripSize, viewport, 8, 6).y).toBe(8);
  });

  it('clamps x when card is near the right edge', () => {
    expect(placeDesktopReactionStrip({ x: 1100, y: 300 }, cardSize, stripSize, viewport, 8, 6).x).toBe(932); // 1200 - 260 - 8
  });
});
