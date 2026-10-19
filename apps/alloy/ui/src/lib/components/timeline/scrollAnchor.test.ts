import { describe, expect, it } from 'vitest';
import { offsetForBottomGap } from './scrollAnchor';

describe('offsetForBottomGap', () => {
  it('stays pinned to the end when the viewport shrinks from the bottom', () => {
    // 2000px of content, 800px viewport at the end -> keyboard leaves 500px.
    expect(offsetForBottomGap(2000, 500, 0)).toBe(1500);
  });

  it('keeps the same distance from the end for a user who scrolled up', () => {
    expect(offsetForBottomGap(2000, 500, 300)).toBe(1200);
  });

  it('never goes negative when the content is shorter than the viewport', () => {
    expect(offsetForBottomGap(200, 500, 0)).toBe(0);
    expect(offsetForBottomGap(2000, 500, -40)).toBe(1500);
  });
});
