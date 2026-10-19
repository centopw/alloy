import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';
import { ALBUM_MAX, groupAlbums } from './albums';

function media(id: string, at: number, extra: Record<string, unknown> = {}): TimelineItemDto {
  return {
    internal_id: id,
    event_id: `$${id}`,
    sender: '@a:hs',
    timestamp_millis: at,
    content: { kind: 'Media', data: { media_kind: 'image', body: 'IMG_0001.jpg', uri: 'mxc://x/y', mime_type: 'image/jpeg', size_bytes: 1 } },
    send_state: { status: 'Sent' },
    is_own: false,
    reply_to: null,
    thread_summary: null,
    read_receipts: null,
    reactions: [],
    ...extra,
  } as unknown as TimelineItemDto;
}

const text = (id: string, at: number): TimelineItemDto =>
  ({ ...media(id, at), content: { kind: 'TextMessage', data: { body: 'hi', formatted_body: null } } }) as unknown as TimelineItemDto;

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
    const rows = [media('a', 0), media('b', 1_000), media('c', 200_000), media('d', 201_000, { sender: '@b:hs' })];
    const layout = groupAlbums(rows);
    expect([...layout.leads.keys()]).toEqual(['a']);
    expect(layout.hidden.has('c')).toBe(false);
  });

  it('keeps pictures that carry a caption, reply, reaction or unsent state out of albums', () => {
    const captioned = media('b', 1_000, { content: { kind: 'Media', data: { media_kind: 'image', body: 'Look at this', uri: 'mxc://x/y', mime_type: 'image/jpeg', size_bytes: 1 } } });
    const reacted = media('c', 2_000, { reactions: [{ key: 'x', count: 1, by_me: false, senders: [] }] });
    const sending = media('d', 3_000, { send_state: { status: 'Sending' } });
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
