import { describe, expect, it } from 'vitest';
import { classifySwipe, swipeOffset, swipeTravel } from './swipe';

describe('classifySwipe', () => {
  it('waits until the finger has moved enough to tell', () => {
    expect(classifySwipe(4, 2, 'right')).toBe('undecided');
  });

  it('claims a clearly horizontal drag toward the swipe direction', () => {
    expect(classifySwipe(30, 5, 'right')).toBe('swipe');
    expect(classifySwipe(-30, 5, 'left')).toBe('swipe');
  });

  it('leaves vertical scrolling and diagonal drags alone', () => {
    expect(classifySwipe(8, 40, 'right')).toBe('reject');
    expect(classifySwipe(30, 20, 'right')).toBe('reject');
  });

  it('rejects a drag the opposite way', () => {
    expect(classifySwipe(-30, 2, 'right')).toBe('reject');
    expect(classifySwipe(30, 2, 'left')).toBe('reject');
  });
});

describe('swipeTravel and swipeOffset', () => {
  it('measures travel along the direction and never goes negative', () => {
    expect(swipeTravel(50, 'right')).toBe(50);
    expect(swipeTravel(-50, 'left')).toBe(50);
    expect(swipeTravel(-20, 'right')).toBe(0);
  });

  it('follows the finger up to the threshold, then resists and caps', () => {
    expect(swipeOffset(40, 64)).toBe(40);
    expect(swipeOffset(64, 64)).toBe(64);
    expect(swipeOffset(114, 64)).toBeCloseTo(84);
    expect(swipeOffset(1000, 64)).toBe(96);
  });
});
