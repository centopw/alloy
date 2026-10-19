import { describe, expect, it } from 'vitest';
import type { RoomSummary, TagColour } from '../generated/index.js';
import { displayTags, makeTagId, TAG_COLOUR_VARS, TAG_COLOURS } from './tags.js';

const COLOURS: TagColour[] = ['grey', 'red', 'orange', 'amber', 'green', 'teal', 'blue', 'violet', 'pink'];

describe('makeTagId', () => {
  it('slugifies the name', () => {
    expect(makeTagId('  Work Stuff! ', new Set())).toBe('work-stuff');
  });

  it('folds accents so accented names stay readable', () => {
    expect(makeTagId('Công việc', new Set())).toBe('cong-viec');
  });

  it('falls back to "tag" when no ASCII letters remain', () => {
    expect(makeTagId('!!!', new Set())).toBe('tag');
    expect(makeTagId('   ', new Set())).toBe('tag');
  });

  it('suffixes taken ids with -2, -3, ...', () => {
    expect(makeTagId('Work', new Set(['work']))).toBe('work-2');
    expect(makeTagId('Work', new Set(['work', 'work-2']))).toBe('work-3');
  });
});

describe('displayTags', () => {
  const room = (tags: string[]) => ({ tags }) as unknown as RoomSummary;

  it('keeps catalogue order and appends unknown rooms tag ids as grey entries', () => {
    const defs = [{ id: 'work', name: 'Work', colour: 'blue' as const }];
    const shown = displayTags(defs, [room(['zeta', 'work']), room(['alpha'])]);
    expect(shown).toEqual([
      { id: 'work', name: 'Work', colour: 'blue' },
      { id: 'alpha', name: 'alpha', colour: 'grey' },
      { id: 'zeta', name: 'zeta', colour: 'grey' },
    ]);
  });

  it('returns the catalogue unchanged when no room carries an unknown id', () => {
    const defs = [{ id: 'work', name: 'Work', colour: 'blue' as const }];
    expect(displayTags(defs, [room(['work'])])).toEqual(defs);
  });
});

describe('tag colours', () => {
  it('maps every palette colour to a design token', () => {
    expect([...TAG_COLOURS].sort()).toEqual([...COLOURS].sort());
    for (const colour of COLOURS) {
      expect(TAG_COLOUR_VARS[colour]).toMatch(/^var\(--alloy-/);
    }
  });
});
