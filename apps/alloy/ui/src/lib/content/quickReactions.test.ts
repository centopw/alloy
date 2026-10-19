import { describe, expect, it } from 'vitest';
import {
  DEFAULT_QUICK_REACTIONS,
  MAX_QUICK_REACTIONS,
  addQuickReaction,
  isQuickReactions,
  removeQuickReaction,
} from './quickReactions';

const up = '\u{1F44D}';
const heart = '\u2764\uFE0F';
const fire = '\u{1F525}';

describe('isQuickReactions', () => {
  it('accepts the defaults', () => {
    expect(isQuickReactions([...DEFAULT_QUICK_REACTIONS])).toBe(true);
  });

  it('rejects empty, oversized, duplicated and malformed lists', () => {
    expect(isQuickReactions([])).toBe(false);
    expect(isQuickReactions(Array.from({ length: MAX_QUICK_REACTIONS + 1 }, (_, i) => `e${i}`))).toBe(false);
    expect(isQuickReactions([up, up])).toBe(false);
    expect(isQuickReactions([up, ''])).toBe(false);
    expect(isQuickReactions([up, 7])).toBe(false);
    expect(isQuickReactions('thumbs')).toBe(false);
  });
});

describe('editing the list', () => {
  it('adds a new emoji once and stops at the limit', () => {
    expect(addQuickReaction([up], fire)).toEqual([up, fire]);
    expect(addQuickReaction([up, fire], fire)).toEqual([up, fire]);
    const full = Array.from({ length: MAX_QUICK_REACTIONS }, (_, i) => `e${i}`);
    expect(addQuickReaction(full, fire)).toBe(full);
  });

  it('never removes the last reaction', () => {
    expect(removeQuickReaction([up, heart], up)).toEqual([heart]);
    expect(removeQuickReaction([heart], heart)).toEqual([heart]);
  });
});
