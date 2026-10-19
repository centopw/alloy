import type { TimelineItemDto } from '$lib/generated/protocol';

/**
 * A media item for use in the gallery viewer, with full metadata.
 * `eventId` may be null for items built from inline MediaView props.
 */
export interface MediaItem {
  eventId: string | null;
  uri: string;
  mimeType: string;
  body: string;
  mediaKind: string;
  sender: string;
  senderName: string | null;
  timestampMillis: number;
}

/**
 * Extract all Media content items from a timeline in order, filtering to image/video only.
 * Each item must have a uri to be included.
 */
export function viewerItemsFrom(items: readonly TimelineItemDto[]): MediaItem[] {
  const result: MediaItem[] = [];

  for (const item of items) {
    if (item.content.kind !== 'Media') continue;

    const data = item.content.data;
    const mediaKind = data.media_kind || '';

    // Include only image and video; exclude sticker, audio, file
    if (mediaKind !== 'image' && mediaKind !== 'video') continue;

    // Must have a uri
    if (!data.uri) continue;

    result.push({
      eventId: item.event_id,
      uri: data.uri,
      mimeType: data.mime_type,
      body: data.body || '',
      mediaKind,
      sender: item.sender,
      senderName: item.sender_display_name || null,
      timestampMillis: item.timestamp_millis,
    });
  }

  return result;
}

/**
 * Find the index of an event in a MediaItem array.
 * Returns the index if found; -1 if not found.
 * If eventId is null/undefined and fallbackUri is provided, matches by URI.
 */
export function indexOfEvent(
  items: MediaItem[],
  eventId: string | null | undefined,
  fallbackUri?: string
): number {
  if (eventId) {
    return items.findIndex((item) => item.eventId === eventId);
  }

  if (fallbackUri) {
    return items.findIndex((item) => item.uri === fallbackUri);
  }

  return -1;
}
