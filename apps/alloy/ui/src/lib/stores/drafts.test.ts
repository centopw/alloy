import { afterEach, describe, expect, it, vi } from 'vitest';

const { saveDraft } = vi.hoisted(() => ({ saveDraft: vi.fn(async () => ({})) }));
vi.mock('../ipc/index.js', () => ({ client: { saveDraft } }));

import { draftsStore } from './drafts.svelte';

const ROOM = '!room:example.org';

describe('drafts', () => {
  afterEach(() => {
    draftsStore.reset();
    saveDraft.mockClear();
  });

  it('flushing at once also updates the room-list preview', async () => {
    await draftsStore.flush(ROOM, 'typed in a hurry');
    expect(draftsStore.previewOf(ROOM)).toBe('typed in a hurry');
    expect(saveDraft).toHaveBeenCalledWith(ROOM, 'typed in a hurry', { reply_to: null, edit_of: null });
  });

  it('flushing an empty draft drops the preview', async () => {
    await draftsStore.flush(ROOM, 'first thought');
    await draftsStore.flush(ROOM, '');
    expect(draftsStore.previewOf(ROOM)).toBeNull();
    expect(saveDraft).toHaveBeenLastCalledWith(ROOM, '', { reply_to: null, edit_of: null });
  });

  it('flushing a pending debounce writes once, not twice', async () => {
    draftsStore.save(ROOM, 'pending');
    await draftsStore.flush(ROOM, 'pending');
    expect(draftsStore.previewOf(ROOM)).toBe('pending');
    expect(saveDraft).toHaveBeenCalledTimes(1);
    await new Promise((r) => setTimeout(r, 600));
    expect(saveDraft).toHaveBeenCalledTimes(1);
  });
});
