import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RoomTagDef } from '../generated/index.js';

/**
 * T25: an empty `GetTagCatalog` answer replaced the whole catalogue with nothing, which took the tag
 * chips and an active tag filter off the room list. An empty answer is also what a catalogue the
 * server never stored looks like, so it must not erase a list the client already has.
 */

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('../ipc/index.js', () => ({ client: { invoke: mocks.invoke } }));
vi.mock('./prefs.svelte.js', () => ({ registerFlushHandler: vi.fn() }));

import { tags } from './tags.svelte';
import { refuseTagWrite } from './tags.js';

const work: RoomTagDef = { id: 'work', name: 'Work', colour: 'blue' };
const home: RoomTagDef = { id: 'home', name: 'Home', colour: 'green' };

const answers = (list: RoomTagDef[]) => ({ type: 'TagCatalog', payload: { tags: list } });

beforeEach(() => {
  vi.restoreAllMocks();
  tags.reset();
  mocks.invoke.mockReset();
});

describe('tag catalogue reads', () => {
  it('keeps the loaded catalogue when the server answers with nothing', async () => {
    mocks.invoke.mockResolvedValueOnce(answers([work, home]));
    await tags.load();
    expect(tags.defs).toEqual([work, home]);

    // The reported symptom: a later read came back empty and the chips disappeared.
    mocks.invoke.mockResolvedValueOnce(answers([]));
    await tags.load();
    expect(tags.defs).toEqual([work, home]);
  });

  it('says so in the console when it ignores an empty answer', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mocks.invoke.mockResolvedValueOnce(answers([work]));
    await tags.load();
    mocks.invoke.mockResolvedValueOnce(answers([]));
    await tags.load();
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('keeping the local catalogue'));
  });

  it('accepts an empty answer when there is nothing to keep', async () => {
    mocks.invoke.mockResolvedValueOnce(answers([]));
    await tags.load();
    expect(tags.defs).toEqual([]);
    expect(tags.loaded).toBe(true);
  });

  it('still applies a real change from the server', async () => {
    mocks.invoke.mockResolvedValueOnce(answers([work]));
    await tags.load();
    mocks.invoke.mockResolvedValueOnce(answers([work, home]));
    await tags.load();
    expect(tags.defs).toEqual([work, home]);
    mocks.invoke.mockResolvedValueOnce(answers([home]));
    await tags.load();
    expect(tags.defs).toEqual([home]);
  });

  it('does not resurrect a catalogue the user just deleted', async () => {
    mocks.invoke.mockResolvedValueOnce(answers([work]));
    await tags.load();
    // Deleting the last tag empties the local list first, so the next empty read is consistent.
    mocks.invoke.mockResolvedValueOnce({ type: 'Unit' });
    await tags.remove('work');
    expect(tags.defs).toEqual([]);
    mocks.invoke.mockResolvedValueOnce(answers([]));
    await tags.load();
    expect(tags.defs).toEqual([]);
  });

  it('refuses to save before the catalogue was read', async () => {
    // A save built from an empty local list would overwrite whatever the server holds, so a
    // catalogue that failed to load must not be written back.
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const created = await tags.create('Work', 'blue');
    expect(created).toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalled();

    mocks.invoke.mockResolvedValueOnce(answers([]));
    await tags.load();
    mocks.invoke.mockResolvedValueOnce({ type: 'RoomTagCreated', payload: { tag: work } });
    expect(await tags.create('Work', 'blue')).toEqual(work);
  });

  it('reports a failed read without dropping the list', async () => {
    mocks.invoke.mockResolvedValueOnce(answers([work]));
    await tags.load();
    mocks.invoke.mockRejectedValueOnce(new Error('the server is unreachable'));
    await tags.load();
    expect(tags.defs).toEqual([work]);
    expect(tags.error).toContain('unreachable');
  });
});

describe('catalogue writes', () => {
  it('refuses to save before the catalogue was read', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(await tags.create('Work', 'blue')).toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('before the catalogue was read'));
  });

  it('refuses to rename or delete a tag the catalogue does not define', async () => {
    // An orphan chip comes from a room tag, not from the catalogue; the engine rebuilds the
    // catalogue on delete, so acting on an unknown id drops every entry this client has not read.
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mocks.invoke.mockResolvedValueOnce(answers([work]));
    await tags.load();
    mocks.invoke.mockClear();

    expect(await tags.update('orphan', { name: 'Renamed' })).toBe(false);
    expect(await tags.remove('orphan')).toBe(false);
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(tags.defs).toEqual([work]);
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('refusing to update orphan'));
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('refusing to remove orphan'));
  });

  it('leaves the catalogue untouched when a read fails and an orphan chip is renamed', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('the server is unreachable'));
    await tags.load();
    expect(tags.loaded).toBe(false);

    vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(await tags.update('orphan', { name: 'Renamed' })).toBe(false);
    expect(await tags.create('Work', 'blue')).toBeNull();
    expect(mocks.invoke).toHaveBeenCalledTimes(1); // only the failed read

    // The next successful read still returns the server's catalogue, untouched.
    mocks.invoke.mockResolvedValueOnce(answers([work, home]));
    await tags.load();
    expect(tags.defs).toEqual([work, home]);
  });

  it('writes one tag per command instead of sending a whole list', async () => {
    mocks.invoke.mockResolvedValueOnce(answers([work]));
    await tags.load();
    mocks.invoke.mockClear();

    mocks.invoke.mockResolvedValueOnce({ type: 'RoomTagCreated', payload: { tag: home } });
    expect(await tags.create('Home', 'green')).toEqual(home);
    expect(mocks.invoke).toHaveBeenLastCalledWith({ action: 'CreateRoomTag', params: { name: 'Home', colour: 'green' } });
    expect(tags.defs).toEqual([work, home]);

    mocks.invoke.mockResolvedValueOnce({ type: 'Unit' });
    expect(await tags.update('home', { name: 'House' })).toBe(true);
    expect(mocks.invoke).toHaveBeenLastCalledWith({ action: 'RenameRoomTag', params: { id: 'home', name: 'House', colour: null } });
    expect(tags.defs[1].name).toBe('House');

    mocks.invoke.mockResolvedValueOnce({ type: 'Unit' });
    expect(await tags.move('home', -1)).toBe(true);
    expect(mocks.invoke).toHaveBeenLastCalledWith({ action: 'ReorderRoomTags', params: { ids: ['home', 'work'] } });
    expect(tags.defs.map((d) => d.id)).toEqual(['home', 'work']);
    // A whole-list write is never sent.
    expect(mocks.invoke.mock.calls.some(([c]) => c.action === 'SaveTagCatalog')).toBe(false);
  });

  it('leaves the order alone when the engine refuses a reorder', async () => {
    mocks.invoke.mockResolvedValueOnce(answers([work, home]));
    await tags.load();
    mocks.invoke.mockRejectedValueOnce(new Error('the catalogue changed on another device'));
    expect(await tags.move('home', -1)).toBe(false);
    expect(tags.defs.map((d) => d.id)).toEqual(['work', 'home']);
    expect(tags.error).toContain('another device');
  });

  it('names the operation when a write is refused', () => {
    expect(refuseTagWrite('create', false)).toContain('refusing to create before the catalogue was read');
    expect(refuseTagWrite('update orphan', true, false)).toContain('not in the catalogue');
    expect(refuseTagWrite('create', true)).toBeNull();
  });
});
