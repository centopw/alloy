import { describe, expect, it } from 'vitest';
import { trapTabTarget } from './focusTrap';

function el(name: string): HTMLElement {
  const e = document.createElement('button');
  e.textContent = name;
  return e;
}

describe('trapTabTarget', () => {
  it('wraps Tab on the last focusable back to the first', () => {
    const [a, b, c] = [el('a'), el('b'), el('c')];
    expect(trapTabTarget([a, b, c], c, false)).toBe(a);
  });

  it('wraps Shift+Tab on the first focusable to the last', () => {
    const [a, b, c] = [el('a'), el('b'), el('c')];
    expect(trapTabTarget([a, b, c], a, true)).toBe(c);
  });

  it('lets the browser move between interior elements', () => {
    const [a, b, c] = [el('a'), el('b'), el('c')];
    expect(trapTabTarget([a, b, c], a, false)).toBeNull();
    expect(trapTabTarget([a, b, c], b, true)).toBeNull();
  });

  it('pulls focus back in when it sits outside the surface', () => {
    const [a, b, c] = [el('a'), el('b'), el('c')];
    expect(trapTabTarget([a, b, c], el('outside'), false)).toBe(a);
    expect(trapTabTarget([a, b, c], null, true)).toBe(c);
  });

  it('does nothing when there is nothing focusable', () => {
    expect(trapTabTarget([], null, false)).toBeNull();
  });
});
