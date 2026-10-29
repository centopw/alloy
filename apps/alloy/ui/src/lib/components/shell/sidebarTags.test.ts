import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * #116: the phone room list showed the tag chips but no way to see the grouped list, because
 * grouping only existed inside the sort submenu. The grouping control sits with the chips now, so
 * this reads the component the way a reviewer would.
 */

const sidebar = readFileSync(resolve(process.cwd(), 'src/lib/components/shell/Sidebar.svelte'), 'utf8');
const chip = readFileSync(resolve(process.cwd(), 'src/lib/components/tags/TagChip.svelte'), 'utf8');

/** The tag-filter toolbar, so the assertions cannot pass on an unrelated part of the file. */
const toolbar = sidebar.slice(
  sidebar.indexOf('<div class="tag-filter"'),
  sidebar.indexOf('{#if tags.error')
);

describe('room-list tag grouping', () => {
  it('offers the grouping control beside the tag chips', () => {
    expect(toolbar).toContain('label="Group"');
    expect(toolbar).toContain("pressed={prefs.roomSort === 'tag'}");
    expect(toolbar).toContain("prefs.set('roomSort', prefs.roomSort === 'tag' ? 'activity' : 'tag')");
  });

  it('keeps the chips and the control on the phone as well', () => {
    // The row is gated on having tags, never on the layout: a phone rendered it and still offered
    // no grouping affordance.
    expect(toolbar.length).toBeGreaterThan(0);
    expect(toolbar).not.toContain('isPhone');
  });

  it('groups into one section per tag when the preference says so', () => {
    expect(sidebar).toContain("{#if prefs.roomSort === 'tag'}");
    expect(sidebar).toContain('groupRoomsByTag(list, tagDefs)');
  });

  it('replaces the pinned/rooms/people sections instead of nesting tag groups in them', () => {
    // Splitting the same rooms into Pinned/Rooms/People first repeated every tag heading once per
    // outer section: a room tagged Work appeared under a second "Work" heading.
    const grouped = sidebar.slice(
      sidebar.indexOf("{#if prefs.roomSort === 'tag'}"),
      sidebar.indexOf('{:else if prefs.roomListSections}')
    );
    expect(grouped.length).toBeGreaterThan(0);
    expect(grouped).toContain('{@render roomList([...pinned, ...unpinned])}');
    expect(grouped).not.toContain('aria-label="Pinned"');
    expect(grouped).not.toContain('aria-label="People"');
  });

  it('lets a chip carry hover text the label alone cannot', () => {
    expect(chip).toContain('title?: string;');
    expect(chip).toContain('{title}');
  });
});
