import { describe, expect, it } from 'vitest';
import type { TimelineItemDto } from '$lib/generated';
import {
  ANNOUNCE_MAX_HOLD_MS,
  ANNOUNCE_SETTLE_MS,
  AnnouncementBatcher,
  CURSOR_PAGE_ROWS,
  describeRow,
  formatAnnouncement,
  isAnnounceableArrival,
  isCursorRow,
  messagePreview,
  moveCursorPosition,
  readReducedMotionSources,
  shortenPreview,
  smoothScrollAllowed,
  timelineActionForKey,
  type AnnouncementLabels,
  type RowDescriptionLabels,
} from './a11y';

function row(
  id: string,
  kind = 'TextMessage',
  options: { own?: boolean; body?: string; mediaKind?: string } = {},
): TimelineItemDto {
  const { own = false, body = 'hello there', mediaKind } = options;
  const content =
    kind === 'TextMessage'
      ? { kind, data: { body, formatted_body: null, html_sanitized: true } }
      : kind === 'Media'
        ? { kind, data: { uri: `mxc://${id}`, mime_type: 'image/png', size_bytes: 1, body, media_kind: mediaKind } }
        : kind === 'RoomState'
          ? { kind, data: { state_type: 'membership', content_json: '{}' } }
          : kind === 'DayDivider'
            ? { kind, data: { timestamp_millis: 0 } }
            : { kind };
  return {
    internal_id: id,
    event_id: `$${id}`,
    sender: '@mai:example.org',
    timestamp_millis: 0,
    is_own: own,
    reply_to: null,
    thread_summary: null,
    read_receipts: null,
    content,
  } as unknown as TimelineItemDto;
}

/** Labels that make the expected string obvious without depending on the real catalogs. */
const labels: AnnouncementLabels = {
  senderName: (item) => item.sender_display_name ?? item.sender,
  preview: (item) => messagePreview(item, (i) => (i.content.kind === 'Media' ? 'Photo' : 'Message')),
  single: (sender, preview) => `${sender}: ${preview}`,
  many: (count, sender, preview) => `${count} new messages, latest from ${sender}: ${preview}`,
};

const rowLabels: RowDescriptionLabels = {
  senderName: (item) => item.sender_display_name ?? item.sender,
  time: () => '3:42 PM',
  preview: (item) => messagePreview(item, () => 'Photo'),
  description: (sender, time, preview) => `${sender}, ${time}: ${preview}`,
};

describe('row kinds', () => {
  it('gives the keyboard cursor every message body kind and nothing else', () => {
    expect(isCursorRow(row('a'))).toBe(true);
    expect(isCursorRow(row('b', 'Media'))).toBe(true);
    expect(isCursorRow(row('c', 'Poll'))).toBe(true);
    expect(isCursorRow(row('d', 'Location'))).toBe(true);
    expect(isCursorRow(row('e', 'Redacted'))).toBe(true);
    expect(isCursorRow(row('f', 'UnableToDecrypt'))).toBe(true);
    expect(isCursorRow(row('g', 'DayDivider'))).toBe(false);
    expect(isCursorRow(row('h', 'RoomState'))).toBe(false);
    expect(isCursorRow(row('i', 'Virtual'))).toBe(false);
    expect(isCursorRow(row('j', 'Reaction'))).toBe(false);
  });

  it('announces other people’s messages, never own echoes or state events', () => {
    expect(isAnnounceableArrival(row('a'))).toBe(true);
    expect(isAnnounceableArrival(row('b', 'Media'))).toBe(true);
    expect(isAnnounceableArrival(row('c', 'Poll'))).toBe(true);
    expect(isAnnounceableArrival(row('d', 'Location'))).toBe(true);
    expect(isAnnounceableArrival(row('e', 'TextMessage', { own: true }))).toBe(false);
    expect(isAnnounceableArrival(row('f', 'RoomState'))).toBe(false);
    expect(isAnnounceableArrival(row('g', 'DayDivider'))).toBe(false);
  });
});

describe('timelineActionForKey', () => {
  it('maps the message-cursor keys', () => {
    expect(timelineActionForKey('ArrowUp')).toBe('previous');
    expect(timelineActionForKey('ArrowDown')).toBe('next');
    expect(timelineActionForKey('PageUp')).toBe('page-up');
    expect(timelineActionForKey('PageDown')).toBe('page-down');
    expect(timelineActionForKey('Home')).toBe('first');
    expect(timelineActionForKey('End')).toBe('last');
  });

  it('opens the message actions with Enter, Space or Shift+F10', () => {
    expect(timelineActionForKey('Enter')).toBe('open-actions');
    expect(timelineActionForKey(' ')).toBe('open-actions');
    expect(timelineActionForKey('F10', { shift: true })).toBe('open-actions');
  });

  it('closes an open menu with Escape', () => {
    expect(timelineActionForKey('Escape')).toBe('dismiss-actions');
  });

  it('leaves browser and OS shortcuts alone', () => {
    expect(timelineActionForKey('Home', { ctrl: true })).toBeNull();
    expect(timelineActionForKey('ArrowUp', { meta: true })).toBeNull();
    expect(timelineActionForKey('End', { alt: true })).toBeNull();
    expect(timelineActionForKey('PageDown', { ctrl: true })).toBeNull();
  });

  it('ignores shifted navigation and unbound keys', () => {
    expect(timelineActionForKey('ArrowUp', { shift: true })).toBeNull();
    expect(timelineActionForKey('F10')).toBeNull();
    expect(timelineActionForKey('a')).toBeNull();
    expect(timelineActionForKey('Tab')).toBeNull();
  });
});

describe('moveCursorPosition', () => {
  it('walks one row at a time and clamps at both ends', () => {
    expect(moveCursorPosition(3, 10, 'previous')).toBe(2);
    expect(moveCursorPosition(3, 10, 'next')).toBe(4);
    expect(moveCursorPosition(0, 10, 'previous')).toBe(0);
    expect(moveCursorPosition(9, 10, 'next')).toBe(9);
  });

  it('pages by CURSOR_PAGE_ROWS and clamps', () => {
    expect(moveCursorPosition(0, 40, 'page-down')).toBe(CURSOR_PAGE_ROWS);
    expect(moveCursorPosition(39, 40, 'page-down')).toBe(39);
    expect(moveCursorPosition(39, 40, 'page-up')).toBe(39 - CURSOR_PAGE_ROWS);
    expect(moveCursorPosition(30, 40, 'page-up', 50)).toBe(0);
  });

  it('jumps to the ends', () => {
    expect(moveCursorPosition(12, 20, 'first')).toBe(0);
    expect(moveCursorPosition(12, 20, 'last')).toBe(19);
  });

  it('has no position in an empty list', () => {
    expect(moveCursorPosition(4, 0, 'next')).toBe(0);
  });
});

describe('shortenPreview', () => {
  it('collapses whitespace to one line', () => {
    expect(shortenPreview('  hello\n\n  there \t friend ')).toBe('hello there friend');
  });

  it('keeps short bodies whole', () => {
    expect(shortenPreview('short message')).toBe('short message');
  });

  it('shortens long bodies with an ellipsis and never splits a surrogate pair', () => {
    const long = 'a'.repeat(200);
    const shortened = shortenPreview(long, 20);
    expect(Array.from(shortened)).toHaveLength(20);
    expect(shortened.endsWith('…')).toBe(true);

    const emoji = '👍'.repeat(30);
    const emojiShortened = shortenPreview(emoji, 4);
    expect(emojiShortened).toBe('👍👍👍…');
  });
});

describe('messagePreview', () => {
  it('uses the body of a text message', () => {
    expect(messagePreview(row('a', 'TextMessage', { body: '  hi\nthere  ' }), () => 'Message')).toBe('hi there');
  });

  it('uses the localized label when a text message has no body', () => {
    expect(messagePreview(row('a', 'TextMessage', { body: '   ' }), () => 'Message')).toBe('Message');
  });

  it('describes non-text messages with the localized label', () => {
    expect(messagePreview(row('a', 'Media', { body: 'cat.png', mediaKind: 'image' }), () => 'Photo')).toBe('Photo');
  });
});

describe('describeRow', () => {
  it('names the row with sender, time and preview', () => {
    const item = row('a', 'TextMessage', { body: 'hello there' });
    item.sender_display_name = 'Mai';
    expect(describeRow(item, rowLabels)).toBe('Mai, 3:42 PM: hello there');
  });
});

describe('formatAnnouncement', () => {
  it('announces one message as sender and preview', () => {
    expect(formatAnnouncement([row('a')], labels)).toBe('@mai:example.org: hello there');
  });

  it('coalesces a burst into one line naming the newest message', () => {
    const batch = [row('a'), row('b'), row('c', 'TextMessage', { body: 'latest news' })];
    expect(formatAnnouncement(batch, labels)).toBe('3 new messages, latest from @mai:example.org: latest news');
  });

  it('never announces own messages or non-message rows', () => {
    expect(formatAnnouncement([], labels)).toBeNull();
    expect(formatAnnouncement([row('a', 'TextMessage', { own: true })], labels)).toBeNull();
    expect(formatAnnouncement([row('a', 'RoomState'), row('b', 'DayDivider')], labels)).toBeNull();
  });

  it('skips non-announceable rows inside a batch', () => {
    const batch = [row('a', 'RoomState'), row('b'), row('c', 'TextMessage', { own: true })];
    expect(formatAnnouncement(batch, labels)).toBe('@mai:example.org: hello there');
  });
});

describe('AnnouncementBatcher', () => {
  it('turns a burst into exactly one announcement', () => {
    const batcher = new AnnouncementBatcher();

    expect(batcher.push([row('a')], 0)).toBe(ANNOUNCE_SETTLE_MS);
    // Messages keep arriving inside the settle window: the announce still happens once.
    expect(batcher.push([row('b')], 200)).toBe(ANNOUNCE_SETTLE_MS);
    expect(batcher.push([row('c')], 600)).toBe(ANNOUNCE_SETTLE_MS);
    expect(batcher.pending).toBe(3);

    expect(batcher.flush({ muted: false, labels })).toBe(
      '3 new messages, latest from @mai:example.org: hello there',
    );
    expect(batcher.pending).toBe(0);
    expect(batcher.flush({ muted: false, labels })).toBeNull();
  });

  it('announces a quiet single message as one line', () => {
    const batcher = new AnnouncementBatcher();
    batcher.push([row('a')], 0);
    expect(batcher.flush({ muted: false, labels })).toBe('@mai:example.org: hello there');
  });

  it('drops the pending batch when announcements are muted', () => {
    const batcher = new AnnouncementBatcher();
    batcher.push([row('a')], 0);
    expect(batcher.flush({ muted: true, labels })).toBeNull();
    expect(batcher.pending).toBe(0);
    expect(batcher.flush({ muted: false, labels })).toBeNull();
  });

  it('ignores own and non-message arrivals', () => {
    const batcher = new AnnouncementBatcher();
    expect(batcher.push([row('a', 'TextMessage', { own: true })], 0)).toBeNull();
    expect(batcher.push([row('b', 'RoomState')], 0)).toBeNull();
    expect(batcher.pending).toBe(0);
    expect(batcher.flush({ muted: false, labels })).toBeNull();
  });

  it('forces a flush after the maximum hold even when messages keep arriving', () => {
    const batcher = new AnnouncementBatcher();
    batcher.push([row('a')], 0);
    // 1500 ms in: settle would be 2400, the 2000 ms hold wins.
    expect(batcher.push([row('b')], 1500)).toBe(ANNOUNCE_MAX_HOLD_MS - 1500);
    // 1900 ms in: the hold is 100 ms away.
    expect(batcher.push([row('c')], 1900)).toBe(100);
    expect(batcher.delayUntil(2050)).toBe(0);
  });

  it('resets a pending batch without announcing it', () => {
    const batcher = new AnnouncementBatcher();
    batcher.push([row('a')], 0);
    batcher.reset();
    expect(batcher.pending).toBe(0);
    expect(batcher.delayUntil(0)).toBeNull();
    expect(batcher.flush({ muted: false, labels })).toBeNull();
  });
});

describe('reduced-motion smooth-scroll gate', () => {
  it('animates only when neither source asks for reduced motion', () => {
    expect(smoothScrollAllowed({ system: false, app: false })).toBe(true);
    expect(smoothScrollAllowed({ system: true, app: false })).toBe(false);
    expect(smoothScrollAllowed({ system: false, app: true })).toBe(false);
    expect(smoothScrollAllowed({ system: true, app: true })).toBe(false);
  });

  it('reads the OS preference through matchMedia and the app flag from the root element', () => {
    const reduced = {
      matchMedia: () => ({ matches: true }),
    };
    const root = { getAttribute: () => 'true' };
    expect(readReducedMotionSources(reduced, root)).toEqual({ system: true, app: true });
  });

  it('reports no preference when the DOM APIs are missing or throw', () => {
    expect(readReducedMotionSources(undefined, undefined)).toEqual({ system: false, app: false });
    expect(
      readReducedMotionSources(
        {
          matchMedia: () => {
            throw new Error('blocked');
          },
        },
        {
          getAttribute: () => {
            throw new Error('blocked');
          },
        },
      ),
    ).toEqual({ system: false, app: false });
    expect(readReducedMotionSources({}, { getAttribute: () => 'false' })).toEqual({ system: false, app: false });
  });
});
