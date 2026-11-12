import { afterEach, describe, expect, it, vi } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn(async () => ({ type: 'DraftSaved' })) }));
vi.mock('../ipc/index.js', () => ({ client: { invoke } }));

import { draftsStore } from './drafts.svelte';

const ROOM = 'chat-1';

describe('drafts', () => {
  afterEach(() => {
    draftsStore.reset();
    invoke.mockClear();
  });

  it('flushing at once also updates the room-list preview', async () => {
    await draftsStore.flush(ROOM, 'typed in a hurry');
    expect(draftsStore.previewOf(ROOM)).toBe('typed in a hurry');
    expect(invoke).toHaveBeenCalledWith({
      action: 'SaveDraft',
      params: { room_id: ROOM, text: 'typed in a hurry', reply_to: null },
    });
  });

  it('carries the reply target with the draft', async () => {
    await draftsStore.flush(ROOM, 'answering', 'evt-9');
    expect(invoke).toHaveBeenCalledWith({
      action: 'SaveDraft',
      params: { room_id: ROOM, text: 'answering', reply_to: 'evt-9' },
    });
  });

  it('flushing an empty draft drops the preview', async () => {
    await draftsStore.flush(ROOM, 'first thought');
    await draftsStore.flush(ROOM, '');
    expect(draftsStore.previewOf(ROOM)).toBeNull();
    expect(invoke).toHaveBeenLastCalledWith({
      action: 'SaveDraft',
      params: { room_id: ROOM, text: '', reply_to: null },
    });
  });

  it('flushing a pending debounce writes once, not twice', async () => {
    vi.useFakeTimers();
    draftsStore.save(ROOM, 'pending');
    await draftsStore.flush(ROOM, 'pending');
    expect(draftsStore.previewOf(ROOM)).toBe('pending');
    expect(invoke).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(600);
    expect(invoke).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});
