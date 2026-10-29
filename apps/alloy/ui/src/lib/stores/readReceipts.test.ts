import { beforeEach, describe, expect, it, vi } from 'vitest';
import { timelineStore } from './timeline.svelte';
import type { TimelineItemDto } from '../generated/index.js';

/**
 * #T9b: the read receipt must be published, retryable and honest.
 *
 * The old path marked the event as done *before* the request and swallowed the error, so a failed
 * receipt was never retried and never reported. These tests drive `markRead()` directly, which is
 * what the timeline triggers on room open, arrival-at-bottom, foreground and own send.
 */

const ipc = vi.hoisted(() => ({ markRead: vi.fn() }));
const rooms = vi.hoisted(() => ({ markRead: vi.fn() }));
const settings = vi.hoisted(() => ({ showReadReceipts: true }));

vi.mock('../ipc/index.js', () => ({
  client: { markRead: ipc.markRead, unsubscribe: vi.fn() },
  applyDiff: vi.fn(),
}));
vi.mock('./rooms.svelte.js', () => ({ roomsStore: { markRead: rooms.markRead } }));
vi.mock('./prefs.svelte.js', () => ({
  prefs: {
    get showReadReceipts() {
      return settings.showReadReceipts;
    },
  },
}));

const item = (eventId: string | null, own = false) =>
  ({ event_id: eventId, is_own: own }) as unknown as TimelineItemDto;

function visible(state: 'visible' | 'hidden') {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
}

async function openRoom(ids: (string | null)[], ownLast = false) {
  await timelineStore.close();
  timelineStore.roomId = '!room:alloy.chat';
  timelineStore.items = ids.map((id, i) => item(id, ownLast && i === ids.length - 1));
}

beforeEach(async () => {
  ipc.markRead.mockReset();
  rooms.markRead.mockReset();
  settings.showReadReceipts = true;
  visible('visible');
  await timelineStore.close();
});

describe('markRead', () => {
  it('publishes the newest confirmed event and does not repeat it', async () => {
    ipc.markRead.mockResolvedValue({ type: 'ReadMarked' });
    await openRoom(['$a', '$b']);

    await timelineStore.markRead();
    expect(ipc.markRead).toHaveBeenCalledWith('!room:alloy.chat', '$b');
    expect(timelineStore.lastReceiptEvent).toBe('$b');
    expect(rooms.markRead).toHaveBeenCalledWith('!room:alloy.chat');

    await timelineStore.markRead();
    expect(ipc.markRead).toHaveBeenCalledTimes(1);
  });

  it('skips local echoes that have no event id yet', async () => {
    ipc.markRead.mockResolvedValue({ type: 'ReadMarked' });
    await openRoom(['$a', null]);

    await timelineStore.markRead();
    expect(ipc.markRead).toHaveBeenCalledWith('!room:alloy.chat', '$a');
  });

  it('reports a failed receipt and leaves the event retryable', async () => {
    ipc.markRead.mockRejectedValueOnce(new Error('M_FORBIDDEN'));
    await openRoom(['$a', '$b']);

    await timelineStore.markRead();
    expect(timelineStore.lastReceiptEvent).toBeNull();
    expect(timelineStore.receiptError).toBe('M_FORBIDDEN');
    // A receipt that never left the device must not clear the unread badge.
    expect(rooms.markRead).not.toHaveBeenCalled();

    ipc.markRead.mockResolvedValueOnce({ type: 'ReadMarked' });
    await timelineStore.markRead();
    expect(ipc.markRead).toHaveBeenCalledTimes(2);
    expect(timelineStore.lastReceiptEvent).toBe('$b');
    expect(timelineStore.receiptError).toBeNull();
  });

  it('never publishes while the document is hidden', async () => {
    ipc.markRead.mockResolvedValue({ type: 'ReadMarked' });
    await openRoom(['$a']);
    visible('hidden');

    await timelineStore.markRead();
    expect(ipc.markRead).not.toHaveBeenCalled();
    expect(timelineStore.lastReceiptEvent).toBeNull();
  });

  it('stops publishing when read receipts are turned off, but still clears the room', async () => {
    ipc.markRead.mockResolvedValue({ type: 'ReadMarked' });
    await openRoom(['$a']);
    settings.showReadReceipts = false;

    await timelineStore.markRead();
    expect(ipc.markRead).not.toHaveBeenCalled();
    expect(rooms.markRead).toHaveBeenCalledWith('!room:alloy.chat');
  });

  it('shares one request between overlapping triggers', async () => {
    const pending = Promise.withResolvers<{ type: 'ReadMarked' }>();
    ipc.markRead.mockReturnValueOnce(pending.promise);
    await openRoom(['$a']);

    const first = timelineStore.markRead();
    const second = timelineStore.markRead();
    expect(ipc.markRead).toHaveBeenCalledTimes(1);
    pending.resolve({ type: 'ReadMarked' });
    await Promise.all([first, second]);
    expect(timelineStore.lastReceiptEvent).toBe('$a');
  });

  it('forgets the receipt when the room closes', async () => {
    ipc.markRead.mockResolvedValue({ type: 'ReadMarked' });
    await openRoom(['$a']);
    await timelineStore.markRead();
    expect(timelineStore.lastReceiptEvent).toBe('$a');

    await timelineStore.close();
    expect(timelineStore.lastReceiptEvent).toBeNull();
    expect(timelineStore.receiptError).toBeNull();
  });

  it('does nothing without an open room', async () => {
    await timelineStore.markRead();
    expect(ipc.markRead).not.toHaveBeenCalled();
  });
});
