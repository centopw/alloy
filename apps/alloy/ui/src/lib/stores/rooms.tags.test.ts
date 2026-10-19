import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RoomSummary } from '../generated/index.js';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../ipc/index.js', () => ({ client: { invoke }, applyDiff: vi.fn() }));

import { roomsStore } from './rooms.svelte.js';
import { tags } from './tags.svelte.js';

const ROOM = '!general:alloy.chat';

function room(tagIds: string[]): RoomSummary {
  return { id: ROOM, tags: tagIds } as RoomSummary;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

describe('RoomsStore.setRoomTag', () => {
  beforeEach(() => {
    invoke.mockReset();
    roomsStore.rooms = [room(['work'])];
    tags.error = null;
  });

  it('shows the assignment before the engine answers', async () => {
    const reply = deferred<{ type: 'Unit' }>();
    invoke.mockReturnValue(reply.promise);

    const pending = roomsStore.setRoomTag(ROOM, 'fun', true);
    expect(roomsStore.rooms[0].tags).toEqual(['work', 'fun']);

    reply.resolve({ type: 'Unit' });
    await expect(pending).resolves.toBe(true);
    expect(roomsStore.rooms[0].tags).toEqual(['work', 'fun']);
    expect(invoke).toHaveBeenCalledWith({
      action: 'SetRoomTag',
      params: { room_id: ROOM, tag_id: 'fun', enabled: true },
    });
  });

  it('restores the previous tags and reports the error when the engine refuses', async () => {
    invoke.mockRejectedValue(new Error('denied'));

    await expect(roomsStore.setRoomTag(ROOM, 'work', false)).resolves.toBe(false);
    expect(roomsStore.rooms[0].tags).toEqual(['work']);
    expect(tags.error).not.toBeNull();
  });

  it('does not duplicate a tag the room already carries', async () => {
    invoke.mockResolvedValue({ type: 'Unit' });

    await roomsStore.setRoomTag(ROOM, 'work', true);
    expect(roomsStore.rooms[0].tags).toEqual(['work']);
  });
});
