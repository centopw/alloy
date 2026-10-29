import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';

/**
 * T10b UI: a room's timeline is kept for reopening, the first `Reset` after a reopen is reconciled
 * into what the cache already holds instead of clearing it, the scroll offset is restored, and the
 * least recently used room past the cache limit is closed in the engine.
 */

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  subscribe: vi.fn(),
  unsubscribe: vi.fn(),
  subscribeTimeline: vi.fn(),
  handlers: [] as ((e: unknown) => void)[],
}));

vi.mock('../ipc/index.js', () => ({
  client: {
    invoke: mocks.invoke,
    subscribe: (fn: (e: unknown) => void) => {
      mocks.handlers.push(fn);
      return () => {};
    },
    unsubscribe: vi.fn().mockResolvedValue(undefined),
    subscribeTimeline: mocks.subscribeTimeline,
    markRead: vi.fn(),
  },
  applyDiff: (target: unknown[], diff: { type: string; content?: { values?: unknown[] } }) => {
    if (diff.type === 'Append' && diff.content?.values) target.push(...diff.content.values);
  },
}));
vi.mock('./uploads.svelte.js', () => ({ uploadsStore: { cancel: vi.fn() } }));
vi.mock('./rooms.svelte.js', () => ({ roomsStore: { markRead: vi.fn() } }));
vi.mock('./prefs.svelte.js', () => ({ prefs: { showReadReceipts: true } }));
vi.mock('../content/receipts.js', () => ({ lastOwnEventId: () => null, readersByEvent: () => new Map() }));
vi.mock('../platform/files.js', () => ({ voiceExtension: vi.fn(), writeTempFile: vi.fn() }));
vi.mock('./subscription.js', () => ({
  Subscription: class {
    subscriptionId = 'sub';
    constructor(public key: (e: unknown) => string, public push: (e: unknown) => void) {}
    attach() {}
    close() {}
  },
  FrameBatcher: class {
    constructor(private run: (events: unknown[]) => void) {}
    push(e: unknown) {
      this.run([e]);
    }
    clear() {}
  },
}));

import { mergeTimelineReset, timelineStore, TIMELINE_CACHE_LIMIT } from './timeline.svelte';

function item(id: string, ts: number, eventId: string | null = `$e${id}`): TimelineItemDto {
  return {
    internal_id: id,
    event_id: eventId,
    sender: '@a:x',
    timestamp_millis: ts,
    content: { kind: 'TextMessage', data: { body: id, formatted_body: null, html_sanitized: true } },
    send_state: { status: 'Sent', data: { event_id: eventId ?? '' } },
    is_own: false,
    reply_to: null,
    thread_summary: null,
    read_receipts: null,
    mentions_me: false,
  } as unknown as TimelineItemDto;
}

beforeEach(() => {
  mocks.invoke.mockReset().mockResolvedValue({ type: 'TimelineClosed', payload: { room_id: '!r', closed: true } });
  mocks.subscribeTimeline.mockReset().mockResolvedValue({
    type: 'TimelineSubscribed',
    payload: { subscription_id: 'sub' },
  });
  mocks.handlers.length = 0;
  timelineStore.items = [];
  timelineStore.roomId = null;
  timelineStore.pendingScroll = null;
});

describe('mergeTimelineReset', () => {
  it('keeps cached history older than the reset window, without duplicates', () => {
    const cached = [item('a', 1), item('b', 2), item('c', 3)];
    const reset = [item('c', 3), item('d', 4)];
    expect(mergeTimelineReset(cached, reset).map((i) => i.internal_id)).toEqual(['a', 'b', 'c', 'd']);
  });

  it('keeps a local echo the reset does not know', () => {
    const cached = [item('a', 1), item('echo', 9, null)];
    const reset = [item('a', 1), item('b', 2)];
    expect(mergeTimelineReset(cached, reset).map((i) => i.internal_id)).toEqual(['a', 'b', 'echo']);
  });

  it('does not resurrect a cached row that the reset moved or removed inside its window', () => {
    const cached = [item('a', 1), item('b', 2)];
    const reset = [item('a', 1)];
    // b is newer than the reset's first item, so it is not "older history" and must not come back.
    expect(mergeTimelineReset(cached, reset).map((i) => i.internal_id)).toEqual(['a']);
  });
});

describe('cached rooms', () => {
  it('restores the cached list, its start flag and its scroll offset', async () => {
    await timelineStore.open('!one:x');
    timelineStore.items = [item('a', 1)];
    timelineStore.rememberScroll(240);
    // Opening another room closes this one, which is what caches it.
    await timelineStore.open('!two:x');
    expect(timelineStore.items).toEqual([]);
    await timelineStore.open('!one:x');
    expect(timelineStore.items.map((i) => i.internal_id)).toEqual(['a']);
    expect(timelineStore.pendingScroll).toBe(240);
  });

  it('closes the least recently used room in the engine once the limit is passed', async () => {
    for (let i = 0; i <= TIMELINE_CACHE_LIMIT; i++) {
      await timelineStore.open(`!room${i}:x`);
      timelineStore.items = [item(`m${i}`, i)];
      await timelineStore.close();
    }
    const closed = mocks.invoke.mock.calls
      .filter(([c]) => c.action === 'CloseTimeline')
      .map(([c]) => c.params.room_id);
    expect(closed).toContain('!room0:x');
    expect(closed).not.toContain(`!room${TIMELINE_CACHE_LIMIT}:x`);
  });

  it('drops every cached room and closes each on sign-out', async () => {
    await timelineStore.open('!one:x');
    timelineStore.items = [item('a', 1)];
    await timelineStore.close();
    timelineStore.clearCache();
    await timelineStore.open('!one:x');
    expect(timelineStore.items).toEqual([]);
    expect(mocks.invoke.mock.calls.some(([c]) => c.action === 'CloseTimeline' && c.params.room_id === '!one:x')).toBe(true);
  });
});
