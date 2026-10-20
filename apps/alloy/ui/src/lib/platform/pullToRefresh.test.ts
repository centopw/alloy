import { describe, expect, it, vi } from 'vitest';
import { PULL_THRESHOLD_PX, pullToRefresh, shouldRefresh } from './pullToRefresh';

function touch(type: string, x: number, y: number) {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, 'touches', { value: type === 'touchend' ? [] : [{ clientX: x, clientY: y }] });
  return event;
}

function pull(node: HTMLElement, from: [number, number], to: [number, number]) {
  node.dispatchEvent(touch('touchstart', ...from));
  node.dispatchEvent(touch('touchmove', ...to));
  node.dispatchEvent(touch('touchend', ...to));
}

describe('shouldRefresh', () => {
  it('needs a long enough pull that started at the top', () => {
    expect(shouldRefresh(PULL_THRESHOLD_PX, true)).toBe(true);
    expect(shouldRefresh(PULL_THRESHOLD_PX - 1, true)).toBe(false);
    expect(shouldRefresh(PULL_THRESHOLD_PX * 2, false)).toBe(false);
  });
});

describe('pullToRefresh', () => {
  it('refreshes after a long pull down from the top', () => {
    const node = document.createElement('div');
    const onrefresh = vi.fn();
    pullToRefresh(node, { onrefresh });
    pull(node, [100, 50], [100, 50 + PULL_THRESHOLD_PX + 10]);
    expect(onrefresh).toHaveBeenCalledOnce();
  });

  it('ignores short pulls, sideways drags, a scrolled list and a disabled action', () => {
    const onrefresh = vi.fn();
    const node = document.createElement('div');
    pullToRefresh(node, { onrefresh });
    pull(node, [100, 50], [100, 90]);
    pull(node, [100, 50], [260, 160]);
    node.scrollTop = 40;
    Object.defineProperty(node, 'scrollTop', { value: 40, configurable: true });
    pull(node, [100, 50], [100, 200]);

    const off = document.createElement('div');
    pullToRefresh(off, { onrefresh, enabled: false });
    pull(off, [100, 50], [100, 200]);
    expect(onrefresh).not.toHaveBeenCalled();
  });
});
