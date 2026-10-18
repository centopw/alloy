import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '../generated/index.js';
import { threadReplies, threadRootItem } from './thread.svelte';

const item = (internal_id: string, event_id: string | null): TimelineItemDto =>
  ({
    internal_id,
    event_id,
    sender: '@alice:alloy.chat',
    timestamp_millis: 0,
    content: { kind: 'TextMessage', data: { body: internal_id, formatted_body: null, html_sanitized: true } },
    send_state: null,
    is_own: false,
    reply_to: null,
    thread_summary: null,
    read_receipts: null,
  }) as unknown as TimelineItemDto;

describe('threadReplies', () => {
  it('drops the root and keeps replies in order', () => {
    const items = [item('root', '$root'), item('r1', '$r1'), item('r2', '$r2')];
    expect(threadReplies(items, '$root').map((i) => i.internal_id)).toEqual(['r1', 'r2']);
  });

  it('keeps local echoes that have no event id yet', () => {
    const items = [item('root', '$root'), item('local', null)];
    expect(threadReplies(items, '$root').map((i) => i.internal_id)).toEqual(['local']);
  });

  it('returns every item when the root has not arrived', () => {
    const items = [item('r1', '$r1')];
    expect(threadReplies(items, '$root')).toHaveLength(1);
  });
});

describe('threadRootItem', () => {
  it('finds the root by event id', () => {
    const items = [item('r1', '$r1'), item('root', '$root')];
    expect(threadRootItem(items, '$root')?.internal_id).toBe('root');
  });

  it('returns null while the root is missing', () => {
    expect(threadRootItem([item('r1', '$r1')], '$root')).toBeNull();
  });
});
