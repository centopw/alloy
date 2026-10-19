import { describe, expect, it } from 'vitest';
import { viewerItemsFrom, indexOfEvent } from './mediaItems';
import type { TimelineItemContent, TimelineItemDto } from '$lib/generated/protocol';

const mockMedia = (
  data: Partial<Extract<TimelineItemContent, { kind: 'Media' }>['data']>
): TimelineItemContent => ({
  kind: 'Media',
  data: {
    uri: 'mxc://example.org/test',
    mime_type: 'image/jpeg',
    size_bytes: 1000,
    thumbnail_uri: null,
    body: 'test',
    media_kind: 'image',
    ...data,
  },
});

const mockItem = (
  overrides: Partial<TimelineItemDto> = {}
): TimelineItemDto => ({
  internal_id: 'id-1',
  event_id: 'event-1',
  sender: '@alice:example.org',
  timestamp_millis: 1000,
  content: { kind: 'TextMessage', data: { body: 'text', formatted_body: null, html_sanitized: false } },
  send_state: { status: 'Sent', data: { event_id: 'event-1' } },
  is_own: false,
  reply_to: null,
  thread_summary: null,
  read_receipts: null,
  ...overrides,
});

describe('viewerItemsFrom', () => {
  it('extracts image items from timeline', () => {
    const items = [
      mockItem({
        event_id: 'img-1',
        content: mockMedia({
          uri: 'mxc://example.org/abc',
          mime_type: 'image/jpeg',
          body: 'photo.jpg',
          media_kind: 'image',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({
      eventId: 'img-1',
      uri: 'mxc://example.org/abc',
      mimeType: 'image/jpeg',
      body: 'photo.jpg',
      mediaKind: 'image',
    });
  });

  it('extracts video items from timeline', () => {
    const items = [
      mockItem({
        event_id: 'vid-1',
        content: mockMedia({
          uri: 'mxc://example.org/vid',
          mime_type: 'video/mp4',
          body: 'clip.mp4',
          media_kind: 'video',
          width: 1920,
          height: 1080,
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(1);
    expect(result[0].mediaKind).toBe('video');
  });

  it('excludes audio items', () => {
    const items = [
      mockItem({
        content: mockMedia({
          uri: 'mxc://example.org/audio',
          mime_type: 'audio/mp3',
          media_kind: 'audio',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(0);
  });

  it('excludes sticker items', () => {
    const items = [
      mockItem({
        content: mockMedia({
          uri: 'mxc://example.org/sticker',
          mime_type: 'image/png',
          media_kind: 'sticker',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(0);
  });

  it('excludes file items', () => {
    const items = [
      mockItem({
        content: mockMedia({
          uri: 'mxc://example.org/file',
          mime_type: 'application/pdf',
          media_kind: 'file',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(0);
  });

  it('excludes items without uri', () => {
    const items = [
      mockItem({
        content: mockMedia({
          uri: '',
          media_kind: 'image',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(0);
  });

  it('skips non-Media items', () => {
    const items = [
      mockItem({
        content: { kind: 'TextMessage', data: { body: 'text', formatted_body: null, html_sanitized: false } },
      }),
      mockItem({
        event_id: 'img-1',
        content: mockMedia({
          uri: 'mxc://example.org/abc',
          media_kind: 'image',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(1);
    expect(result[0].eventId).toBe('img-1');
  });

  it('preserves timeline order', () => {
    const items = [
      mockItem({
        event_id: 'img-1',
        timestamp_millis: 1000,
        sender_display_name: 'Alice',
        content: mockMedia({
          uri: 'mxc://example.org/abc',
          media_kind: 'image',
        }),
      }),
      mockItem({
        event_id: 'img-2',
        timestamp_millis: 2000,
        sender_display_name: 'Bob',
        content: mockMedia({
          uri: 'mxc://example.org/def',
          media_kind: 'image',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result).toHaveLength(2);
    expect(result[0].eventId).toBe('img-1');
    expect(result[1].eventId).toBe('img-2');
  });

  it('handles empty timeline', () => {
    expect(viewerItemsFrom([])).toEqual([]);
  });

  it('uses body fallback when absent', () => {
    const items = [
      mockItem({
        content: mockMedia({
          body: undefined,
          uri: 'mxc://example.org/abc',
          media_kind: 'image',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result[0].body).toBe('');
  });

  it('preserves display name', () => {
    const items = [
      mockItem({
        sender_display_name: 'Alice',
        content: mockMedia({
          uri: 'mxc://example.org/abc',
          media_kind: 'image',
        }),
      }),
    ];

    const result = viewerItemsFrom(items);
    expect(result[0].senderName).toBe('Alice');
  });
});

describe('indexOfEvent', () => {
  const items = [
    { eventId: 'a', uri: 'uri-a', mimeType: 'image/jpeg', body: '', mediaKind: 'image', sender: '@a:example.org', senderName: null, timestampMillis: 1000 },
    { eventId: 'b', uri: 'uri-b', mimeType: 'image/jpeg', body: '', mediaKind: 'image', sender: '@b:example.org', senderName: null, timestampMillis: 2000 },
    { eventId: 'c', uri: 'uri-c', mimeType: 'image/jpeg', body: '', mediaKind: 'image', sender: '@c:example.org', senderName: null, timestampMillis: 3000 },
  ];

  it('finds item by event id', () => {
    expect(indexOfEvent(items, 'b')).toBe(1);
  });

  it('returns -1 when event id not found', () => {
    expect(indexOfEvent(items, 'z')).toBe(-1);
  });

  it('uses fallback uri when event id is null', () => {
    expect(indexOfEvent(items, null, 'uri-b')).toBe(1);
  });

  it('uses fallback uri when event id is undefined', () => {
    expect(indexOfEvent(items, undefined, 'uri-c')).toBe(2);
  });

  it('returns -1 when fallback uri not found', () => {
    expect(indexOfEvent(items, null, 'uri-missing')).toBe(-1);
  });

  it('prefers event id over fallback uri', () => {
    expect(indexOfEvent(items, 'b', 'uri-c')).toBe(1);
  });

  it('returns -1 when both event id and fallback are missing', () => {
    expect(indexOfEvent(items, null)).toBe(-1);
  });

  it('handles empty list', () => {
    expect(indexOfEvent([], 'a')).toBe(-1);
  });
});
