import { describe, expect, it } from 'vitest';
import {
  classifySwipe,
  computePinchScale,
  clampPan,
  zoomAroundPoint,
  dismissOpacity,
} from './viewerGestures';

describe('classifySwipe', () => {
  it('classifies downward swipe as dismiss when dy >= 120 and |dy| > 1.5 * |dx|', () => {
    expect(classifySwipe(10, 140)).toBe('dismiss');
    expect(classifySwipe(-20, 120)).toBe('dismiss');
  });

  it('rejects downward swipe when horizontal displacement is too large', () => {
    expect(classifySwipe(100, 120)).toBe('none');
  });

  it('classifies fast downward fling as dismiss', () => {
    expect(classifySwipe(10, 50, 0, 1.2)).toBe('dismiss');
  });

  it('does not classify upward swipe as dismiss', () => {
    expect(classifySwipe(0, -140)).toBe('none');
  });

  it('classifies leftward swipe as horizontal-next when |dx| >= 60 and |dx| > 1.5 * |dy|', () => {
    expect(classifySwipe(-80, 10)).toBe('horizontal-next');
    expect(classifySwipe(-60, 20)).toBe('horizontal-next');
  });

  it('classifies rightward swipe as horizontal-prev when |dx| >= 60 and |dx| > 1.5 * |dy|', () => {
    expect(classifySwipe(80, 10)).toBe('horizontal-prev');
    expect(classifySwipe(60, 20)).toBe('horizontal-prev');
  });

  it('classifies fast horizontal flings', () => {
    expect(classifySwipe(-40, 5, -0.8, 0)).toBe('horizontal-next');
    expect(classifySwipe(40, 5, 0.8, 0)).toBe('horizontal-prev');
  });

  it('rejects ambiguous diagonal swipes', () => {
    expect(classifySwipe(60, 50)).toBe('none');
    expect(classifySwipe(-60, -50)).toBe('none');
  });

  it('returns none for small displacements', () => {
    expect(classifySwipe(10, 10)).toBe('none');
    expect(classifySwipe(0, 0)).toBe('none');
  });
});

describe('computePinchScale', () => {
  it('scales proportionally to pointer distance ratio', () => {
    expect(computePinchScale(1, 100, 200)).toBe(2);
    expect(computePinchScale(2, 100, 150)).toBe(3);
  });

  it('clamps scale between min and max (defaults 1..4)', () => {
    expect(computePinchScale(1, 100, 50)).toBe(1); // Clamped at min 1
    expect(computePinchScale(1, 100, 500)).toBe(4); // Clamped at max 4
  });

  it('respects custom min/max bounds', () => {
    expect(computePinchScale(1, 100, 300, 0.5, 2.5)).toBe(2.5);
    expect(computePinchScale(1, 100, 50, 0.5, 2.5)).toBe(0.5);
  });

  it('handles non-positive initial distance safely', () => {
    expect(computePinchScale(2, 0, 100)).toBe(2);
    expect(computePinchScale(5, -10, 100)).toBe(4);
  });
});

describe('clampPan', () => {
  const bounds = {
    containerWidth: 800,
    containerHeight: 600,
    contentWidth: 800,
    contentHeight: 600,
  };

  it('locks pan to 0 when scale is 1', () => {
    const clamped = clampPan({ x: 100, y: -50 }, 1, bounds);
    expect(clamped).toEqual({ x: 0, y: 0 });
  });

  it('allows panning up to max bounds when zoomed', () => {
    // At scale 2, content size is 1600x1200.
    // maxPanX = (1600 - 800) / 2 = 400.
    // maxPanY = (1200 - 600) / 2 = 300.
    const withinBounds = clampPan({ x: 200, y: -150 }, 2, bounds);
    expect(withinBounds).toEqual({ x: 200, y: -150 });

    const exceedsBounds = clampPan({ x: 600, y: -500 }, 2, bounds);
    expect(exceedsBounds).toEqual({ x: 400, y: -300 });

    const exceedsNegative = clampPan({ x: -500, y: 400 }, 2, bounds);
    expect(exceedsNegative).toEqual({ x: -400, y: 300 });
  });

  it('locks pan on axis if content on that axis is smaller than container', () => {
    const wideBounds = {
      containerWidth: 1000,
      containerHeight: 400,
      contentWidth: 400,
      contentHeight: 400,
    };
    // At scale 2: content width = 800 <= 1000 -> maxPanX = 0.
    // content height = 800 > 400 -> maxPanY = 200.
    const clamped = clampPan({ x: 50, y: 150 }, 2, wideBounds);
    expect(clamped).toEqual({ x: 0, y: 150 });
  });
});

describe('zoomAroundPoint', () => {
  it('preserves focal point when scaling up', () => {
    // Center of container is (0, 0).
    // Focal point is (100, 100).
    // Current pan is (0, 0) at scale 1.
    // New scale is 2.
    // Pan should shift so point (100, 100) stays under cursor:
    // x = 100 - (100 - 0) * 2 = -100.
    const pan = zoomAroundPoint({ x: 0, y: 0 }, 1, 2, { x: 100, y: 100 });
    expect(pan).toEqual({ x: -100, y: -100 });
  });

  it('returns to 0 pan when zooming back to scale 1 around the same point', () => {
    const pan = zoomAroundPoint({ x: -100, y: -100 }, 2, 1, { x: 100, y: 100 });
    expect(pan).toEqual({ x: 0, y: 0 });
  });

  it('handles zero or negative current scale safely', () => {
    const initial = { x: 10, y: 20 };
    expect(zoomAroundPoint(initial, 0, 2, { x: 50, y: 50 })).toEqual(initial);
  });
});

describe('dismissOpacity', () => {
  it('returns 1 for zero or negative displacement', () => {
    expect(dismissOpacity(0)).toBe(1);
    expect(dismissOpacity(-50)).toBe(1);
  });

  it('decreases smoothly with downward displacement', () => {
    expect(dismissOpacity(150, 300)).toBeCloseTo(0.65);
    expect(dismissOpacity(300, 300)).toBeCloseTo(0.3);
  });

  it('clamps at minimum opacity for displacement exceeding maxDistance', () => {
    expect(dismissOpacity(600, 300)).toBeCloseTo(0.3);
  });
});
