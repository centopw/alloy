import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';
import { badgeLabel, countArrivals } from './arrivals';

function row(
  id: string,
  own: boolean,
  kind: 'TextMessage' | 'Media' | 'Location' | 'Deleted' | 'System' | 'DayDivider',
  body = 'hello'
): TimelineItemDto {
  const content: TimelineItemDto['content'] =
    kind === 'TextMessage'
      ? { kind: 'TextMessage', data: { body, formatted_body: null, html_sanitized: false } }
      : kind === 'Media'
        ? { kind: 'Media', data: { source: { media_id: id }, mime_type: 'image/png', size_bytes: 1, body, media_kind: 'image', width: null, height: null, duration_ms: null, voice: false } }
        : kind === 'Location'
          ? { kind: 'Location', data: { geo_uri: 'geo:1,2', description: null } }
          : kind === 'Deleted'
            ? { kind: 'Deleted' }
            : kind === 'System'
              ? { kind: 'System', data: { text: body } }
              : { kind: 'DayDivider', data: { timestamp_millis: 0 } };
  return {
    internal_id: id,
    event_id: `$${id}`,
    sender: 'u-other',
    sender_display_name: null,
    sender_avatar: null,
    timestamp_millis: 0,
    content,
    send_state: { status: 'Sent', data: { event_id: `$${id}` } },
    is_own: own,
    reply_to: null,
    read_by: [],
    edited: false,
    reactions: [],
    transaction_id: null,
  };
}

describe('countArrivals', () => {
  it("ignores the user's own messages and rows that carry no message text", () => {
    const result = countArrivals([
      row('a', true, 'TextMessage'),
      row('b', false, 'System'),
      row('c', false, 'DayDivider'),
      row('d', false, 'Deleted'),
      row('e', false, 'TextMessage'),
    ]);
    expect(result).toEqual({ count: 1, mentions: [] });
  });

  it('lists mentioning messages oldest first', () => {
    const result = countArrivals(
      [
        row('a', false, 'TextMessage', 'hey @Mara Vogel'),
        row('b', false, 'Media', 'cat.png'),
        row('c', false, 'TextMessage', '@Mara Vogel again'),
      ],
      ['Mara Vogel']
    );
    expect(result).toEqual({ count: 3, mentions: ['$a', '$c'] });
  });

  it('counts captions as text, so a picture nobody captioned with the name is not a mention', () => {
    const result = countArrivals([row('a', false, 'Media', '@Mara Vogel look')], ['Mara Vogel']);
    expect(result).toEqual({ count: 1, mentions: ['$a'] });
  });

  it('reports no mentions when the account has no display name', () => {
    const result = countArrivals([row('a', false, 'TextMessage', '@Mara hi')]);
    expect(result).toEqual({ count: 1, mentions: [] });
  });
});

describe('badgeLabel', () => {
  it('caps large counts', () => {
    expect(badgeLabel(7)).toBe('7');
    expect(badgeLabel(100)).toBe('99+');
  });
});
