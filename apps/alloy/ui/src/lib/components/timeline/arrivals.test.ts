import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';
import { badgeLabel, countArrivals } from './arrivals';

function row(id: string, own: boolean, kind: string, mentions = false): TimelineItemDto {
  return {
    internal_id: id,
    event_id: `$${id}`,
    is_own: own,
    mentions_me: mentions,
    content: { kind, data: {} },
  } as unknown as TimelineItemDto;
}

describe('countArrivals', () => {
  it("ignores the user's own messages and non-message rows", () => {
    const result = countArrivals([row('a', true, 'TextMessage'), row('b', false, 'RoomState'), row('c', false, 'TextMessage')]);
    expect(result).toEqual({ count: 1, mentions: [] });
  });

  it('lists mentioning messages oldest first', () => {
    const result = countArrivals([row('a', false, 'TextMessage', true), row('b', false, 'Media'), row('c', false, 'TextMessage', true)]);
    expect(result).toEqual({ count: 3, mentions: ['$a', '$c'] });
  });
});

describe('badgeLabel', () => {
  it('caps large counts', () => {
    expect(badgeLabel(7)).toBe('7');
    expect(badgeLabel(100)).toBe('99+');
  });
});
