import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';
import { lastOwnEventId, readersByEvent } from './receipts';

function item(id: string, sender: string, own: boolean, receipts: string[] | null): TimelineItemDto {
  return {
    internal_id: id,
    event_id: `$${id}`,
    sender,
    timestamp_millis: 0,
    content: { kind: 'TextMessage', data: { body: id, formatted_body: null } },
    send_state: { status: 'Sent' },
    is_own: own,
    reply_to: null,
    thread_summary: null,
    read_receipts: receipts,
    reactions: [],
  } as unknown as TimelineItemDto;
}

describe('readersByEvent', () => {
  it('counts a receipt on a later event as having read the earlier one', () => {
    const items = [
      item('a', '@me:hs', true, ['@me:hs']),
      item('b', '@bob:hs', false, ['@bob:hs']),
      item('c', '@me:hs', true, []),
    ];
    const readers = readersByEvent(items);
    expect(readers.get('$a')).toEqual(['@bob:hs']);
    expect(readers.get('$c')).toEqual([]);
    expect(readers.has('$b')).toBe(false);
  });

  it('excludes your own receipts and tolerates missing receipt lists', () => {
    const items = [item('a', '@me:hs', true, null), item('b', '@me:hs', true, ['@me:hs', '@cy:hs'])];
    const readers = readersByEvent(items);
    expect(readers.get('$a')).toEqual(['@cy:hs']);
  });
});

describe('lastOwnEventId', () => {
  it('finds your newest message', () => {
    const items = [item('a', '@me:hs', true, []), item('b', '@bob:hs', false, [])];
    expect(lastOwnEventId(items)).toBe('$a');
    expect(lastOwnEventId([])).toBeNull();
  });
});
