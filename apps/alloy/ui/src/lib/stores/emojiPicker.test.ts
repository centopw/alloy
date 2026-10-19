import { afterEach, describe, expect, it, vi } from 'vitest';
import { emojiPicker } from './emojiPicker.svelte';

const anchor = { x: 10, y: 20, width: 30, height: 40 };

afterEach(() => {
  emojiPicker.close();
});

describe('emojiPicker store', () => {
  it('opens with the anchor rectangle', () => {
    emojiPicker.open({ anchor, onpick: () => {} });
    expect(emojiPicker.isOpen).toBe(true);
    expect(emojiPicker.anchor).toEqual(anchor);
  });

  it('delivers the pick and closes unless keepOpen is set', () => {
    const onpick = vi.fn();
    emojiPicker.open({ anchor, onpick });
    emojiPicker.pick('\u{1F44D}');
    expect(onpick).toHaveBeenCalledWith('\u{1F44D}');
    expect(emojiPicker.isOpen).toBe(false);
    expect(emojiPicker.anchor).toBeNull();
  });

  it('stays open after a pick when keepOpen is set', () => {
    const onpick = vi.fn();
    emojiPicker.open({ anchor, onpick, keepOpen: true });
    emojiPicker.pick('\u{1F600}');
    expect(onpick).toHaveBeenCalledWith('\u{1F600}');
    expect(emojiPicker.isOpen).toBe(true);
  });

  it('calls onclose once when closed, even if closed again', () => {
    const onclose = vi.fn();
    emojiPicker.open({ anchor, onpick: () => {}, onclose });
    emojiPicker.close();
    emojiPicker.close();
    expect(onclose).toHaveBeenCalledTimes(1);
    expect(emojiPicker.isOpen).toBe(false);
  });

  it('calls onclose when a pick closes the picker', () => {
    const onclose = vi.fn();
    emojiPicker.open({ anchor, onpick: () => {}, onclose });
    emojiPicker.pick('\u{2615}');
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it('ignores a pick while closed', () => {
    expect(() => emojiPicker.pick('\u{2615}')).not.toThrow();
    expect(emojiPicker.isOpen).toBe(false);
  });
});
