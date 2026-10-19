import type { RoomSummary, RoomTagDef, TagColour } from '../generated/index.js';

/** Design token each tag colour paints with. Reuses the status and user palettes. */
export const TAG_COLOUR_VARS: Record<TagColour, string> = {
  grey: 'var(--alloy-color-status-offline)',
  red: 'var(--alloy-color-status-error)',
  orange: 'var(--alloy-color-status-mention)',
  amber: 'var(--alloy-color-status-warning)',
  green: 'var(--alloy-color-status-success)',
  teal: 'var(--alloy-color-user-5)',
  blue: 'var(--alloy-color-status-info)',
  violet: 'var(--alloy-color-user-9)',
  pink: 'var(--alloy-color-user-11)',
};

export const TAG_COLOURS = Object.keys(TAG_COLOUR_VARS) as TagColour[];

/**
 * Derives a stable, ASCII tag id from a display name, suffixed with -2, -3, ... when taken.
 * Accents are folded so "Công việc" becomes "cong-viec"; names with no ASCII letters become "tag".
 */
export function makeTagId(name: string, taken: ReadonlySet<string>): string {
  const base =
    name
      .trim()
      .toLowerCase()
      .normalize('NFD')
      .replace(/[\u0300-\u036f]/g, '')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'tag';
  let id = base;
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
  return id;
}

/**
 * Display list of tags: the catalogue in its order, plus a grey entry for every tag id that rooms
 * carry but the catalogue does not define (for example after a delete from another client).
 */
export function displayTags(defs: readonly RoomTagDef[], rooms: readonly RoomSummary[]): RoomTagDef[] {
  const known = new Set(defs.map((d) => d.id));
  const orphans = new Set<string>();
  for (const r of rooms) for (const id of r.tags) if (!known.has(id)) orphans.add(id);
  const extra = [...orphans].sort().map((id) => ({ id, name: id, colour: 'grey' as const }));
  return [...defs, ...extra];
}
