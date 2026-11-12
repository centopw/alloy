/**
 * Demo content for the in-browser engine (`pnpm dev`, screenshots, E2E walks).
 *
 * Everything here speaks protocol v2: local store ids for accounts, chats and messages, opaque
 * remote user ids, and per-network capabilities. Two personal accounts are seeded — Telegram and
 * Messenger — so the room list, account grouping and network badges have something real to show.
 *
 * This module is developer sample content: it is imported only by `src/lib/ipc/mock.ts`, which
 * `index.ts` reaches through a statically-false `import.meta.env.DEV` branch.
 */

import type {
  Account,
  AccountId,
  CapabilitiesDto,
  ChatKind,
  EventId,
  MediaSource,
  NotificationCount,
  RoomId,
  RoomMemberDto,
  RoomSummary,
  RoomTagDef,
  TimelineItemDto,
  TimelineItemPreview,
  UserId,
} from '../generated/index.js';

/** What one mock transport starts with; every collection is fresh per instance. */
export interface DemoState {
  accounts: Account[];
  rooms: RoomSummary[];
  members: Map<RoomId, RoomMemberDto[]>;
  timelines: Map<RoomId, TimelineItemDto[]>;
  tags: RoomTagDef[];
  drafts: Map<RoomId, { text: string; reply_to: EventId | null }>;
  notes: Map<RoomId, string>;
  /** `media_id` → data URL, so `GetMedia` can hand the webview something it can render. */
  media: Map<string, string>;
  /** Remote user id → display name, for profiles the chat list does not already carry. */
  people: Record<UserId, string>;
}

/**
 * Deterministic UUID v4-shaped id for a demo label.
 *
 * The store assigns UUIDs; readable literals would leak into tests and screenshots as obviously
 * synthetic, so the seed derives stable ids from a namespace and a label instead.
 */
export function stableId(namespace: string, label: string): string {
  const key = `${namespace}:${label}`;
  const a = fnv1a(key, 0x9e3779b9);
  const b = fnv1a(key, 0x85ebca6b);
  const c = fnv1a(key, 0xc2b2ae35);
  const d = fnv1a(key, 0x27d4eb2f);
  const variant = ((Number.parseInt(c[0], 16) & 0x3) | 0x8).toString(16);
  return `${a}-${b.slice(0, 4)}-4${b.slice(5, 8)}-${variant}${c.slice(1, 4)}-${c.slice(4, 8)}${d}`;
}

function fnv1a(input: string, seed: number): string {
  let hash = (0x811c9dc5 ^ seed) >>> 0;
  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

/** Local chat ids, exported so tests and scripts can address a seeded chat without a lookup. */
export const DEMO_ROOM = {
  telegramDev: stableId('chat', 'telegram-dev-circle'),
  telegramJonas: stableId('chat', 'telegram-jonas'),
  telegramPriya: stableId('chat', 'telegram-priya'),
  telegramWeekly: stableId('chat', 'telegram-rust-weekly'),
  telegramSam: stableId('chat', 'telegram-sam'),
  messengerLena: stableId('chat', 'messenger-lena'),
  messengerFlat: stableId('chat', 'messenger-flat-4b'),
  messengerTomas: stableId('chat', 'messenger-tomas'),
  messengerCycling: stableId('chat', 'messenger-cycling'),
} as const;

export const DEMO_ACCOUNT = {
  telegram: stableId('account', 'telegram-mara'),
  messenger: stableId('account', 'messenger-mara'),
} as const;

export const DEMO_TAG = {
  work: 'work',
  friends: 'friends',
  home: 'home',
} as const;

const TG_MARA: UserId = 'tg:478200113';
const TG_JONAS: UserId = 'tg:201884557';
const TG_PRIYA: UserId = 'tg:338112904';
const TG_SAM: UserId = 'tg:415902771';
const TG_RUST_WEEKLY: UserId = 'tg:rust-weekly';

const FB_MARA: UserId = 'fb:100048821337';
const FB_LENA: UserId = 'fb:100073211994';
const FB_TOMAS: UserId = 'fb:100084520118';
const FB_IVY: UserId = 'fb:100090077331';

/** Telegram: edits inside 48 h, one reaction per user, no polls or shared locations. */
export const TELEGRAM_CAPABILITIES: CapabilitiesDto = {
  edit: true,
  edit_window_secs: 172_800,
  delete_for_everyone: true,
  reactions: true,
  max_reactions_per_user: 1,
  replies: true,
  threads: false,
  typing: true,
  read_receipts: true,
  delivered_receipts: true,
  voice: true,
  polls: false,
  location: false,
  max_upload_bytes: 2_097_152_000,
  formatted_text: true,
};

/** Messenger: 15-minute edit window, 25 MiB attachments, polls but no shared locations. */
export const MESSENGER_CAPABILITIES: CapabilitiesDto = {
  edit: true,
  edit_window_secs: 900,
  delete_for_everyone: true,
  reactions: true,
  max_reactions_per_user: null,
  replies: true,
  threads: false,
  typing: true,
  read_receipts: true,
  delivered_receipts: true,
  voice: true,
  polls: true,
  location: false,
  max_upload_bytes: 26_214_400,
  formatted_text: false,
};

/** The deterministic test connector: enough to exercise the flows, nothing network-specific. */
export const FAKE_CAPABILITIES: CapabilitiesDto = {
  edit: true,
  edit_window_secs: null,
  delete_for_everyone: true,
  reactions: true,
  max_reactions_per_user: null,
  replies: true,
  threads: false,
  typing: true,
  read_receipts: true,
  delivered_receipts: true,
  voice: true,
  polls: false,
  location: false,
  max_upload_bytes: null,
  formatted_text: false,
};

const CAPABILITIES_BY_NETWORK: Record<Account['network'], CapabilitiesDto> = {
  telegram: TELEGRAM_CAPABILITIES,
  messenger: MESSENGER_CAPABILITIES,
  fake: FAKE_CAPABILITIES,
};

/** Capabilities a freshly connected account of `network` gets. */
export function capabilitiesFor(network: Account['network']): CapabilitiesDto {
  return { ...CAPABILITIES_BY_NETWORK[network] };
}

/**
 * Pinned chats first, archived ones last, then newest activity. The engine's room-list projection
 * keeps this order, so every diff the mock emits preserves it.
 */
export function sortRoomList(rooms: RoomSummary[]): RoomSummary[] {
  return rooms.sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    if (a.archived !== b.archived) return a.archived ? 1 : -1;
    const aTs = a.latest_event?.timestamp_millis ?? 0;
    const bTs = b.latest_event?.timestamp_millis ?? 0;
    if (aTs !== bTs) return bTs - aTs;
    return (a.name ?? '').localeCompare(b.name ?? '');
  });
}

/** Room-list preview of one timeline item; day dividers carry no preview. */
export function previewOf(item: TimelineItemDto): TimelineItemPreview | null {
  const base = { sender: item.sender, timestamp_millis: item.timestamp_millis };
  switch (item.content.kind) {
    case 'TextMessage':
      return { ...base, body: item.content.data.body };
    case 'Media':
      return { ...base, body: item.content.data.body };
    case 'Location':
      return { ...base, body: item.content.data.description ?? 'Location' };
    case 'Deleted':
      return { ...base, body: 'Message deleted' };
    case 'System':
      return { ...base, body: item.content.data.text };
    case 'DayDivider':
      return null;
  }
}

/** Latest preview of a timeline, skipping the day dividers the engine interleaves. */
export function latestPreview(items: TimelineItemDto[]): TimelineItemPreview | null {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const preview = previewOf(items[i]);
    if (preview !== null) return preview;
  }
  return null;
}

/** A date separator for the day `millis` falls on, at local midnight. */
export function dayDivider(millis: number): TimelineItemDto {
  const date = new Date(millis);
  const midnight = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  return {
    internal_id: stableId('divider', String(midnight)),
    event_id: null,
    sender: '',
    sender_display_name: null,
    sender_avatar: null,
    timestamp_millis: midnight,
    content: { kind: 'DayDivider', data: { timestamp_millis: midnight } },
    send_state: { status: 'Sent', data: { event_id: null } },
    is_own: false,
    reply_to: null,
    read_by: [],
    reactions: [],
    edited: false,
    transaction_id: null,
  };
}

/** Inserts a divider before the first item of every local day, oldest first. */
export function withDayDividers(items: TimelineItemDto[]): TimelineItemDto[] {
  const out: TimelineItemDto[] = [];
  let day: string | null = null;
  for (const item of items) {
    const itemDay = new Date(item.timestamp_millis).toDateString();
    if (itemDay !== day) {
      out.push(dayDivider(item.timestamp_millis));
      day = itemDay;
    }
    out.push(item);
  }
  return out;
}

/** An inline SVG picture: demo media the webview can render without an asset protocol. */
export function svgImageDataUrl(title: string, width: number, height: number): string {
  const safeTitle = title
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
  const horizon = Math.round(height * 0.62);
  const svg = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`,
    '<defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">',
    '<stop offset="0" stop-color="#2b3a55"/><stop offset="0.55" stop-color="#d98c5f"/>',
    '<stop offset="1" stop-color="#f2c078"/></linearGradient></defs>',
    `<rect width="${width}" height="${height}" fill="url(#sky)"/>`,
    `<rect y="${horizon}" width="${width}" height="${height - horizon}" fill="#1d2b3a"/>`,
    `<circle cx="${Math.round(width * 0.72)}" cy="${Math.round(horizon * 0.52)}" r="${Math.round(height * 0.07)}" fill="#f8e0b0"/>`,
    `<text x="48" y="${height - 56}" font-family="system-ui, sans-serif" font-size="40" fill="#f5efe6">${safeTitle}</text>`,
    '</svg>',
  ].join('');
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** A plain-text file as a data URL; the file row's download works in a browser dev build. */
export function textFileDataUrl(body: string): string {
  return `data:text/plain;charset=utf-8,${encodeURIComponent(body)}`;
}

/**
 * A silent WAV voice note of `millis` (capped at eight seconds) as a data URL.
 *
 * The demo has no microphone recording to upload, so a voice message has to carry real audio
 * bytes the player can decode; silence at 8 kHz mono is a few kilobytes.
 */
export function wavDataUrl(millis: number): string {
  const rate = 8_000;
  const frames = Math.max(1, Math.min(Math.round((rate * millis) / 1_000), rate * 8));
  const bytes = new Uint8Array(44 + frames);
  const view = new DataView(bytes.buffer);
  const ascii = (offset: number, text: string): void => {
    for (let i = 0; i < text.length; i += 1) view.setUint8(offset + i, text.charCodeAt(i));
  };
  ascii(0, 'RIFF');
  view.setUint32(4, 36 + frames, true);
  ascii(8, 'WAVE');
  ascii(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate, true);
  view.setUint16(32, 1, true);
  view.setUint16(34, 8, true);
  ascii(36, 'data');
  view.setUint32(40, frames, true);
  bytes.fill(128, 44);
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return `data:audio/wav;base64,${btoa(binary)}`;
}

interface TextInit {
  reply_to?: TimelineItemDto | null;
  read_by?: UserId[];
  reactions?: TimelineItemDto['reactions'];
  edited?: boolean;
}

interface MediaInit {
  mime_type: string;
  body: string;
  media_kind: string;
  size_bytes: number;
  width?: number | null;
  height?: number | null;
  duration_ms?: number | null;
  voice?: boolean;
  read_by?: UserId[];
}

/** Builds one seeded demo state. Call once per `MockTransport`. */
export function seedDemo(): DemoState {
  const now = Date.now();
  const media = new Map<string, string>();
  const people: Record<UserId, string> = {
    [TG_MARA]: 'Mara Vogel',
    [TG_JONAS]: 'Jonas Weber',
    [TG_PRIYA]: 'Priya Nair',
    [TG_SAM]: 'Sam Oduya',
    [TG_RUST_WEEKLY]: 'Rust Weekly',
    [FB_MARA]: 'Mara Vogel',
    [FB_LENA]: 'Lena Fischer',
    [FB_TOMAS]: 'Tomás Rivera',
    [FB_IVY]: 'Ivy Chen',
  };

  const text = (
    room: RoomId,
    me: UserId,
    key: string,
    sender: UserId,
    minutesAgo: number,
    body: string,
    init: TextInit = {}
  ): TimelineItemDto => {
    const eventId = stableId(`event:${room}`, key);
    const timestamp = now - minutesAgo * 60_000;
    return {
      internal_id: eventId,
      event_id: eventId,
      sender,
      sender_display_name: people[sender] ?? null,
      sender_avatar: null,
      timestamp_millis: timestamp,
      content: { kind: 'TextMessage', data: { body, formatted_body: null, html_sanitized: true } },
      send_state: { status: 'Sent', data: { event_id: eventId } },
      is_own: sender === me,
      reply_to: init.reply_to ?? null,
      read_by: init.read_by ?? [],
      edited: init.edited ?? false,
      reactions: init.reactions ?? [],
      transaction_id: null,
    };
  };

  const mediaItem = (
    room: RoomId,
    me: UserId,
    key: string,
    sender: UserId,
    minutesAgo: number,
    source: MediaSource,
    init: MediaInit
  ): TimelineItemDto => {
    const eventId = stableId(`event:${room}`, key);
    return {
      internal_id: eventId,
      event_id: eventId,
      sender,
      sender_display_name: people[sender] ?? null,
      sender_avatar: null,
      timestamp_millis: now - minutesAgo * 60_000,
      content: {
        kind: 'Media',
        data: {
          source,
          mime_type: init.mime_type,
          size_bytes: init.size_bytes,
          body: init.body,
          media_kind: init.media_kind,
          width: init.width ?? null,
          height: init.height ?? null,
          duration_ms: init.duration_ms ?? null,
          voice: init.voice ?? false,
        },
      },
      send_state: { status: 'Sent', data: { event_id: eventId } },
      is_own: sender === me,
      reply_to: null,
      read_by: init.read_by ?? [],
      edited: false,
      reactions: [],
      transaction_id: null,
    };
  };

  // === Telegram: Dev Circle ===
  const dev = DEMO_ROOM.telegramDev;
  const devSchemaId = stableId('media', 'telegram-store-schema');
  const devSchema = textFileDataUrl(
    'CREATE TABLE accounts (account_id TEXT PRIMARY KEY, network TEXT NOT NULL);\n' +
      'CREATE TABLE chats (chat_id TEXT PRIMARY KEY, account_id TEXT NOT NULL);\n' +
      'CREATE TABLE messages (message_id TEXT PRIMARY KEY, chat_id TEXT NOT NULL);\n'
  );
  media.set(devSchemaId, devSchema);
  const devReview = text(
    dev,
    TG_MARA,
    'dev-4',
    TG_PRIYA,
    380,
    'Morning. The room-list projector is pushed; pinned rows keep their position.'
  );
  const devMerged = text(dev, TG_MARA, 'dev-6', TG_JONAS, 355, 'Merged. CI is green on all five targets.', {
    reactions: [{ key: '👍', count: 1, senders: [TG_PRIYA], by_me: false }],
  });
  const devTimeline = withDayDividers([
    text(dev, TG_MARA, 'dev-1', TG_JONAS, 1_470, 'Store migration landed: accounts, chats and messages are all in SQLite now.'),
    text(dev, TG_MARA, 'dev-2', TG_PRIYA, 1_460, 'Did the full-text index survive the schema bump?'),
    text(dev, TG_MARA, 'dev-3', TG_JONAS, 1_450, 'It did. Search across all chats stays under 20 ms on the 10k fixture.'),
    devReview,
    text(dev, TG_MARA, 'dev-5', TG_SAM, 375, 'Reviewed it. One note: the archived filter belongs in the projection, not the row.', {
      reply_to: devReview,
    }),
    devMerged,
    mediaItem(dev, TG_MARA, 'dev-7', TG_MARA, 352, { media_id: devSchemaId }, {
      mime_type: 'text/plain',
      body: 'store-schema-v1.sql',
      media_kind: 'file',
      size_bytes: devSchema.length,
      read_by: [TG_JONAS],
    }),
  ]);

  // === Telegram: Jonas Weber (direct) ===
  const jonas = DEMO_ROOM.telegramJonas;
  const jonasTimeline = withDayDividers([
    text(jonas, TG_MARA, 'jonas-1', TG_JONAS, 1_980, 'Do you have a minute for the release checklist tomorrow?'),
    text(jonas, TG_MARA, 'jonas-2', TG_MARA, 1_975, 'Yes, after 10. I will bring the Android package names.', {
      read_by: [TG_JONAS],
    }),
    text(jonas, TG_MARA, 'jonas-3', TG_JONAS, 500, 'Great. The nightly build needs the new signing key before it can publish.'),
    text(jonas, TG_MARA, 'jonas-4', TG_MARA, 480, 'Rotating it now; I will ping you when the artifacts are up.', {
      read_by: [TG_JONAS],
    }),
    text(jonas, TG_MARA, 'jonas-5', TG_JONAS, 470, 'Did you get the build logs? I can send the full trace.'),
  ]);

  // === Telegram: Priya Nair (direct) ===
  const priya = DEMO_ROOM.telegramPriya;
  const priyaTimeline = withDayDividers([
    text(priya, TG_MARA, 'priya-1', TG_MARA, 1_600, 'Sent the fixture list, all five networks covered.', {
      read_by: [TG_PRIYA],
    }),
    text(priya, TG_MARA, 'priya-2', TG_PRIYA, 1_590, 'Perfect, that unblocks the connector tests.'),
  ]);

  // === Telegram: Rust Weekly (channel, archived) ===
  const weekly = DEMO_ROOM.telegramWeekly;
  const weeklyTimeline = withDayDividers([
    text(
      weekly,
      TG_MARA,
      'weekly-1',
      TG_RUST_WEEKLY,
      1_700,
      'Issue 412 — async traits on stable, cargo profiles for release builds, and a new sampling profiler.'
    ),
  ]);

  // === Telegram: Sam Oduya (direct, muted) ===
  const sam = DEMO_ROOM.telegramSam;
  const samTimeline = withDayDividers([
    text(sam, TG_MARA, 'sam-1', TG_SAM, 1_260, 'Ping when the nightly is out. I want to test the Android build on the old tablet.'),
  ]);

  // === Messenger: Lena Fischer (direct) ===
  const lena = DEMO_ROOM.messengerLena;
  const coastId = stableId('media', 'messenger-coast-photo');
  const coast = svgImageDataUrl('Coast at dusk', 1_200, 800);
  media.set(coastId, coast);
  const lenaTimeline = withDayDividers([
    text(lena, FB_MARA, 'lena-1', FB_LENA, 1_740, 'Are you still in town this weekend?'),
    text(lena, FB_MARA, 'lena-2', FB_MARA, 1_735, 'Yes, until Sunday evening.', { read_by: [FB_LENA] }),
    mediaItem(lena, FB_MARA, 'lena-3', FB_LENA, 320, { media_id: coastId }, {
      mime_type: 'image/svg+xml',
      body: 'Coast at dusk',
      media_kind: 'image',
      size_bytes: coast.length,
      width: 1_200,
      height: 800,
    }),
    text(lena, FB_MARA, 'lena-4', FB_LENA, 315, 'The photos from the coast are up, the light was unreal.'),
  ]);

  // === Messenger: Flat 4B (group) ===
  const flat = DEMO_ROOM.messengerFlat;
  // One outgoing message stays in the outbox's failed state so the retry and cancel rows are
  // visible in the demo without waiting for a network fault.
  const flatFailed = text(flat, FB_MARA, 'flat-5', FB_MARA, 700, 'I will pick up the parcel at six.');
  flatFailed.event_id = null;
  flatFailed.send_state = { status: 'Failed', data: { error: 'Network unavailable', can_retry: true } };
  flatFailed.transaction_id = 'txn_demo_failed';
  const flatTimeline = withDayDividers([
    text(flat, FB_MARA, 'flat-1', FB_TOMAS, 1_300, 'The plumber comes at nine on Friday.'),
    text(flat, FB_MARA, 'flat-2', FB_IVY, 1_290, 'I can let him in, I am working from home.'),
    text(flat, FB_MARA, 'flat-3', FB_TOMAS, 800, 'Thanks Ivy.'),
    text(flat, FB_MARA, 'flat-4', FB_IVY, 760, 'Bin night is Thursday this week, the schedule moved.'),
    flatFailed,
  ]);

  // === Messenger: Tomás Rivera (direct, voice note) ===
  const tomas = DEMO_ROOM.messengerTomas;
  const voiceId = stableId('media', 'messenger-tomas-voice');
  const voice = wavDataUrl(3_400);
  media.set(voiceId, voice);
  const tomasTimeline = withDayDividers([
    mediaItem(tomas, FB_MARA, 'tomas-1', FB_TOMAS, 2_160, { media_id: voiceId }, {
      mime_type: 'audio/wav',
      body: 'Voice message',
      media_kind: 'voice',
      size_bytes: voice.length,
      duration_ms: 3_400,
      voice: true,
    }),
    text(tomas, FB_MARA, 'tomas-2', FB_MARA, 2_155, 'Got it. Saturday at ten works.', { read_by: [FB_TOMAS] }),
  ]);

  // === Messenger: Weekend Cycling (group) ===
  const cycling = DEMO_ROOM.messengerCycling;
  const cyclingTimeline = withDayDividers([
    text(
      cycling,
      FB_MARA,
      'cycling-1',
      FB_LENA,
      1_620,
      'Route for Saturday, 62 km, coffee at the halfway point: https://example.org/routes/valley-loop'
    ),
    text(cycling, FB_MARA, 'cycling-2', FB_TOMAS, 1_610, 'That climb after the bridge is brutal, but fine.'),
  ]);

  const timelines = new Map<RoomId, TimelineItemDto[]>([
    [dev, devTimeline],
    [jonas, jonasTimeline],
    [priya, priyaTimeline],
    [weekly, weeklyTimeline],
    [sam, samTimeline],
    [lena, lenaTimeline],
    [flat, flatTimeline],
    [tomas, tomasTimeline],
    [cycling, cyclingTimeline],
  ]);

  const members = new Map<RoomId, RoomMemberDto[]>([
    [
      dev,
      [
        { user_id: TG_MARA, display_name: 'Mara Vogel', avatar: null, role: 'owner' },
        { user_id: TG_JONAS, display_name: 'Jonas Weber', avatar: null, role: 'admin' },
        { user_id: TG_PRIYA, display_name: 'Priya Nair', avatar: null, role: null },
        { user_id: TG_SAM, display_name: 'Sam Oduya', avatar: null, role: null },
      ],
    ],
    [jonas, [
      { user_id: TG_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: TG_JONAS, display_name: 'Jonas Weber', avatar: null, role: null },
    ]],
    [priya, [
      { user_id: TG_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: TG_PRIYA, display_name: 'Priya Nair', avatar: null, role: null },
    ]],
    [weekly, [
      { user_id: TG_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: TG_RUST_WEEKLY, display_name: 'Rust Weekly', avatar: null, role: 'owner' },
    ]],
    [sam, [
      { user_id: TG_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: TG_SAM, display_name: 'Sam Oduya', avatar: null, role: null },
    ]],
    [lena, [
      { user_id: FB_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: FB_LENA, display_name: 'Lena Fischer', avatar: null, role: null },
    ]],
    [flat, [
      { user_id: FB_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: FB_TOMAS, display_name: 'Tomás Rivera', avatar: null, role: 'admin' },
      { user_id: FB_IVY, display_name: 'Ivy Chen', avatar: null, role: null },
    ]],
    [tomas, [
      { user_id: FB_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: FB_TOMAS, display_name: 'Tomás Rivera', avatar: null, role: null },
    ]],
    [cycling, [
      { user_id: FB_MARA, display_name: 'Mara Vogel', avatar: null, role: null },
      { user_id: FB_LENA, display_name: 'Lena Fischer', avatar: null, role: null },
      { user_id: FB_TOMAS, display_name: 'Tomás Rivera', avatar: null, role: null },
      { user_id: FB_IVY, display_name: 'Ivy Chen', avatar: null, role: null },
    ]],
  ]);

  const tags: RoomTagDef[] = [
    { id: DEMO_TAG.work, name: 'Work', colour: 'blue' },
    { id: DEMO_TAG.friends, name: 'Friends', colour: 'violet' },
    { id: DEMO_TAG.home, name: 'Home', colour: 'green' },
  ];

  const drafts = new Map<RoomId, { text: string; reply_to: EventId | null }>([
    [jonas, { text: 'Checking the logs now, the last step timed out on the signing job…', reply_to: null }],
  ]);

  const draftPreview = (roomId: RoomId): string | null => {
    const draft = drafts.get(roomId);
    return draft && draft.text.trim() !== '' ? draft.text.slice(0, 80) : null;
  };

  interface RoomInit {
    id: RoomId;
    account: AccountId;
    network: RoomSummary['network'];
    name: string;
    kind: ChatKind;
    unread: NotificationCount;
    pinned?: boolean;
    archived?: boolean;
    muted_until?: number | null;
    marked_unread?: boolean;
    tags?: string[];
  }

  const room = (init: RoomInit): RoomSummary => ({
    id: init.id,
    account_id: init.account,
    network: init.network,
    name: init.name,
    avatar: null,
    kind: init.kind,
    is_direct: init.kind === 'direct',
    unread_notifications: init.unread,
    latest_event: latestPreview(timelines.get(init.id) ?? []),
    pinned: init.pinned ?? false,
    archived: init.archived ?? false,
    muted_until: init.muted_until ?? null,
    marked_unread: init.marked_unread ?? false,
    tags: init.tags ?? [],
    draft: draftPreview(init.id),
  });

  const rooms = sortRoomList([
    room({
      id: dev,
      account: DEMO_ACCOUNT.telegram,
      network: 'telegram',
      name: 'Dev Circle',
      kind: 'group',
      unread: { highlight_count: 0, notification_count: 2 },
      pinned: true,
      tags: [DEMO_TAG.work],
    }),
    room({
      id: jonas,
      account: DEMO_ACCOUNT.telegram,
      network: 'telegram',
      name: 'Jonas Weber',
      kind: 'direct',
      unread: { highlight_count: 1, notification_count: 3 },
    }),
    room({
      id: priya,
      account: DEMO_ACCOUNT.telegram,
      network: 'telegram',
      name: 'Priya Nair',
      kind: 'direct',
      unread: { highlight_count: 0, notification_count: 0 },
    }),
    room({
      id: weekly,
      account: DEMO_ACCOUNT.telegram,
      network: 'telegram',
      name: 'Rust Weekly',
      kind: 'channel',
      unread: { highlight_count: 0, notification_count: 4 },
      archived: true,
    }),
    room({
      id: sam,
      account: DEMO_ACCOUNT.telegram,
      network: 'telegram',
      name: 'Sam Oduya',
      kind: 'direct',
      unread: { highlight_count: 0, notification_count: 1 },
      muted_until: now + 2 * 3_600_000,
      tags: [DEMO_TAG.friends],
    }),
    room({
      id: lena,
      account: DEMO_ACCOUNT.messenger,
      network: 'messenger',
      name: 'Lena Fischer',
      kind: 'direct',
      unread: { highlight_count: 1, notification_count: 2 },
    }),
    room({
      id: flat,
      account: DEMO_ACCOUNT.messenger,
      network: 'messenger',
      name: 'Flat 4B',
      kind: 'group',
      unread: { highlight_count: 0, notification_count: 0 },
      marked_unread: true,
      tags: [DEMO_TAG.home],
    }),
    room({
      id: tomas,
      account: DEMO_ACCOUNT.messenger,
      network: 'messenger',
      name: 'Tomás Rivera',
      kind: 'direct',
      unread: { highlight_count: 0, notification_count: 0 },
    }),
    room({
      id: cycling,
      account: DEMO_ACCOUNT.messenger,
      network: 'messenger',
      name: 'Weekend Cycling',
      kind: 'group',
      unread: { highlight_count: 0, notification_count: 5 },
      tags: [DEMO_TAG.friends],
    }),
  ]);

  const accounts: Account[] = [
    {
      id: DEMO_ACCOUNT.telegram,
      network: 'telegram',
      remote_user_id: TG_MARA,
      display_name: 'Mara Vogel',
      avatar: null,
      state: 'active',
      capabilities: { ...TELEGRAM_CAPABILITIES },
    },
    {
      id: DEMO_ACCOUNT.messenger,
      network: 'messenger',
      remote_user_id: FB_MARA,
      display_name: 'Mara Vogel',
      avatar: null,
      state: 'active',
      capabilities: { ...MESSENGER_CAPABILITIES },
    },
  ];

  return {
    accounts,
    rooms,
    members,
    timelines,
    tags,
    drafts,
    notes: new Map<RoomId, string>(),
    media,
    people,
  };
}
