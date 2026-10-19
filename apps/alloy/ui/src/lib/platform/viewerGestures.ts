export type SwipeAction = 'horizontal-next' | 'horizontal-prev' | 'dismiss' | 'none';

export interface PanOffset {
  x: number;
  y: number;
}

export interface Bounds {
  containerWidth: number;
  containerHeight: number;
  contentWidth: number;
  contentHeight: number;
}

/**
 * Classify a touch/pointer swipe gesture based on displacement and velocity.
 * Downward swipe with dy >= 120 and |dy| > 1.5 * |dx| dismisses.
 * Horizontal swipe with |dx| >= 60 and |dx| > 1.5 * |dy| moves to next/previous item.
 */
export function classifySwipe(
  dx: number,
  dy: number,
  vx = 0,
  vy = 0
): SwipeAction {
  const absDx = Math.abs(dx);
  const absDy = Math.abs(dy);

  // Downward dismiss check
  const isDownwardDominant = absDy > 1.5 * absDx;
  const isDismissDistance = dy >= 120;
  const isDismissFling = dy >= 40 && vy >= 0.8;
  if (isDownwardDominant && (isDismissDistance || isDismissFling)) {
    return 'dismiss';
  }

  // Horizontal navigation check
  const isHorizontalDominant = absDx > 1.5 * absDy;
  const isNavDistance = absDx >= 60;
  const isNavFling = absDx >= 30 && Math.abs(vx) >= 0.5;
  if (isHorizontalDominant && (isNavDistance || isNavFling)) {
    if (dx < 0 || (absDx < 60 && vx < 0)) {
      return 'horizontal-next';
    }
    return 'horizontal-prev';
  }

  return 'none';
}

/**
 * Compute pinch zoom scale clamped between min and max (defaults 1..4).
 */
export function computePinchScale(
  initialScale: number,
  initialDistance: number,
  currentDistance: number,
  min = 1,
  max = 4
): number {
  if (initialDistance <= 0) {
    return Math.min(max, Math.max(min, initialScale));
  }
  const ratio = currentDistance / initialDistance;
  return Math.min(max, Math.max(min, initialScale * ratio));
}

/**
 * Clamp pan offset so the content stays within the container boundaries when scaled.
 * When the scaled content is smaller than the container along an axis, pan on that axis is locked to 0.
 */
export function clampPan(
  pan: PanOffset,
  scale: number,
  bounds: Bounds
): PanOffset {
  const scaledWidth = bounds.contentWidth * scale;
  const scaledHeight = bounds.contentHeight * scale;

  const maxPanX = Math.max(0, (scaledWidth - bounds.containerWidth) / 2);
  const maxPanY = Math.max(0, (scaledHeight - bounds.containerHeight) / 2);

  const rawX = Math.min(maxPanX, Math.max(-maxPanX, pan.x));
  const rawY = Math.min(maxPanY, Math.max(-maxPanY, pan.y));
  const clampedX = Object.is(rawX, -0) ? 0 : rawX;
  const clampedY = Object.is(rawY, -0) ? 0 : rawY;

  return { x: clampedX, y: clampedY };
}

/**
 * Calculate the new pan offset when zooming to a new scale centered around a focal point
 * (given as offset relative to the container center).
 */
export function zoomAroundPoint(
  currentPan: PanOffset,
  currentScale: number,
  newScale: number,
  pointRelativeToCenter: PanOffset
): PanOffset {
  if (currentScale <= 0) return currentPan;
  const ratio = newScale / currentScale;
  const rawX = pointRelativeToCenter.x - (pointRelativeToCenter.x - currentPan.x) * ratio;
  const rawY = pointRelativeToCenter.y - (pointRelativeToCenter.y - currentPan.y) * ratio;
  return {
    x: Object.is(rawX, -0) ? 0 : rawX,
    y: Object.is(rawY, -0) ? 0 : rawY,
  };
}
/**
 * Backdrop opacity calculation for downward dismiss drag.
 * Drops gradually from 1.0 down to a minimum of ~0.3 as dy reaches maxDistance.
 */
export function dismissOpacity(dy: number, maxDistance = 300): number {
  if (dy <= 0) return 1;
  const progress = Math.min(1, dy / maxDistance);
  return 1 - progress * 0.7;
}
