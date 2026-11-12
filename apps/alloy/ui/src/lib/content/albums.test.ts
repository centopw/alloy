import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';
import { ALBUM_MAX, groupAlbums } from './albums';

function media(id: string, at: number, extra: Partial<TimelineItemDto> = {}): TimelineItemDto {
  const item: TimelineItemDto = {
    internal_id: id,
    event_id: `$${id}`,
    sender: 'u-a',
    sender_display_name: null,
    sender_avatar: null,
    timestamp_millis: at,
    content: {
      kind: 'Media',
      data: {
        source: { media_id: `m-${id}` },
        mime_type: 'image/jpeg',
        size_bytes: 1,
        body: 'IMG_0001.jpg',
        media_kind: 'image',
        width: null,
        height: null,
        duration_ms: null,
        voice: false,
      },
    },
    send_state: { status: 'Sent', data: { event_id: `$${id}` } },
    is_own: false,
    reply_to: null,
    read_by: [],
    edited: false,
    reactions: [],
    transaction_id: null,
  };
  return Object.assign(item, extra);
}

const text = (id: string, at: number): TimelineItemDto => ({
  ...media(id, at),
  content: { kind: 'TextMessage', data: { body: 'hi', formatted_body: null, html_sanitized: false } },
});

describe('groupAlbums', () => {
  it('folds consecutive pictures from one sender into the first row', () => {
    const layout = groupAlbums([media('a', 0), media('b', 5_000), media('c', 9_000)]);
    expect([...layout.leads.keys()]).toEqual(['a']);
    expect(layout.leads.get('a')?.map((i) => i.internal_id)).toEqual(['a', 'b', 'c']);
    expect([...layout.hidden]).toEqual(['b', 'c']);
  });

  it('leaves a single picture alone', () => {
    const layout = groupAlbums([media('a', 0), text('t', 1_000), media('b', 2_000)]);
    expect(layout.leads.size).toBe(0);
    expect(layout.hidden.size).toBe(0);
  });

  it('starts a new album after a long gap or a different sender', () => {
    const rows = [media('a', 0), media('b', 1_000), media('c', 200_000), media('d', 201_000, { sender: 'u-b' })];
    const layout = groupAlbums(rows);
    expect([...layout.leads.keys()]).toEqual(['a']);
    expect(layout.hidden.has('c')).toBe(false);
  });

  it('keeps pictures that carry a caption, reply, reaction or unsent state out of albums', () => {
    const captioned = media('b', 1_000, {
      content: {
        kind: 'Media',
        data: {
          source: { media_id: 'm-b' },
          mime_type: 'image/jpeg',
          size_bytes: 1,
          body: 'Look at this',
          media_kind: 'image',
          width: null,
          height: null,
          duration_ms: null,
          voice: false,
        },
      },
    });
    const reacted = media('c', 2_000, { reactions: [{ key: 'x', count: 1, by_me: false, senders: [] }] });
    const sending = media('d', 3_000, { send_state: { status: 'Sending', data: { transaction_id: 'txn-1' } } });
    const layout = groupAlbums([media('a', 0), captioned, reacted, sending, media('e', 4_000)]);
    expect(layout.leads.size).toBe(0);
  });

  it('splits a long run into albums of at most ten', () => {
    const rows = Array.from({ length: ALBUM_MAX + 3 }, (_, i) => media(`m${i}`, i * 1_000));
    const layout = groupAlbums(rows);
    expect(layout.leads.get('m0')).toHaveLength(ALBUM_MAX);
    expect(layout.leads.get(`m${ALBUM_MAX}`)).toHaveLength(3);
  });
});
