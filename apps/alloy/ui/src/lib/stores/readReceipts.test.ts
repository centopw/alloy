import { beforeEach, describe, expect, it, vi } from 'vitest';
import { timelineStore } from './timeline.svelte';
import type { TimelineItemDto } from '../generated/index.js';

/**
 * The read position must be published, retryable and honest.
 *
 * A receipt that has not reached the engine must not clear the unread badge and must stay
 * retryable; these tests drive `markRead()` directly, which is what the timeline triggers on room
 * open, arrival-at-bottom, foreground and own send.
 */

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
const rooms = vi.hoisted(() => ({ markRead: vi.fn() }));

vi.mock('../ipc/index.js', () => ({
  client: { invoke: ipc.invoke, unsubscribe: vi.fn() },
  applyDiff: vi.fn(),
}));
vi.mock('./rooms.svelte.js', () => ({ roomsStore: { markRead: rooms.markRead } }));

const item = (eventId: string | null, own = false) =>
  ({ event_id: eventId, is_own: own }) as unknown as TimelineItemDto;

function visible(state: 'visible' | 'hidden') {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
}

async function openRoom(ids: (string | null)[], ownLast = false) {
  await timelineStore.close();
  timelineStore.roomId = 'r-room';
  timelineStore.items = ids.map((id, i) => item(id, ownLast && i === ids.length - 1));
}

const marked = { type: 'ReadMarked' as const };

beforeEach(async () => {
  ipc.invoke.mockReset();
  rooms.markRead.mockReset();
  visible('visible');
  await timelineStore.close();
});

describe('markRead', () => {
  it('publishes the newest confirmed event and does not repeat it', async () => {
    ipc.invoke.mockResolvedValue(marked);
    await openRoom(['$a', '$b']);

    await timelineStore.markRead();
    expect(ipc.invoke).toHaveBeenCalledWith({
      action: 'MarkRead',
      params: { room_id: 'r-room', event_id: '$b' },
    });
    expect(timelineStore.lastReceiptEvent).toBe('$b');
    expect(rooms.markRead).toHaveBeenCalledWith('r-room');

    await timelineStore.markRead();
    expect(ipc.invoke).toHaveBeenCalledTimes(1);
  });

  it('skips local echoes that have no event id yet', async () => {
    ipc.invoke.mockResolvedValue(marked);
    await openRoom(['$a', null]);

    await timelineStore.markRead();
    expect(ipc.invoke).toHaveBeenCalledWith({
      action: 'MarkRead',
      params: { room_id: 'r-room', event_id: '$a' },
    });
  });

  it('reports a failed receipt and leaves the event retryable', async () => {
    ipc.invoke.mockRejectedValueOnce(new Error('sync offline'));
    await openRoom(['$a', '$b']);

    await timelineStore.markRead();
    expect(timelineStore.lastReceiptEvent).toBeNull();
    expect(timelineStore.receiptError).toBe('sync offline');
    // A receipt that never left the device must not clear the unread badge.
    expect(rooms.markRead).not.toHaveBeenCalled();

    ipc.invoke.mockResolvedValueOnce(marked);
    await timelineStore.markRead();
    expect(ipc.invoke).toHaveBeenCalledTimes(2);
    expect(timelineStore.lastReceiptEvent).toBe('$b');
    expect(timelineStore.receiptError).toBeNull();
  });

  it('never publishes while the document is hidden', async () => {
    ipc.invoke.mockResolvedValue(marked);
    await openRoom(['$a']);
    visible('hidden');

    await timelineStore.markRead();
    expect(ipc.invoke).not.toHaveBeenCalled();
    expect(timelineStore.lastReceiptEvent).toBeNull();
  });

  it('shares one request between overlapping triggers', async () => {
    const pending = Promise.withResolvers<{ type: 'ReadMarked' }>();
    ipc.invoke.mockReturnValueOnce(pending.promise);
    await openRoom(['$a']);

    const first = timelineStore.markRead();
    const second = timelineStore.markRead();
    expect(ipc.invoke).toHaveBeenCalledTimes(1);
    pending.resolve(marked);
    await Promise.all([first, second]);
    expect(timelineStore.lastReceiptEvent).toBe('$a');
  });

  it('forgets the receipt when the room closes', async () => {
    ipc.invoke.mockResolvedValue(marked);
    await openRoom(['$a']);
    await timelineStore.markRead();
    expect(timelineStore.lastReceiptEvent).toBe('$a');

    await timelineStore.close();
    expect(timelineStore.lastReceiptEvent).toBeNull();
    expect(timelineStore.receiptError).toBeNull();
  });

  it('does nothing without an open room', async () => {
    await timelineStore.markRead();
    expect(ipc.invoke).not.toHaveBeenCalled();
  });
});
