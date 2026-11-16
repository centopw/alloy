/**
 * In-memory development engine for protocol v2.
 *
 * `pnpm dev`, the screenshot scripts and the E2E walks run the UI without a connector; this class
 * answers the same commands and streams the same diffs the real engine does, from the seeded demo
 * state in `../mock/seed.ts`. It is reached only through `dev-transport.ts`, which `index.ts`
 * imports behind a statically-false `import.meta.env.DEV` branch, so it never ships.
 *
 * Behaviour worth knowing while reading:
 * - login is the connector state machine: Telegram walks phone → code (`12345`) → optional
 *   password (only when the phone ends with `9`), Messenger walks cookies, the fake network walks
 *   phone → code;
 * - sending is the outbox: `MessageQueued` answers the command, the local echo arrives as a
 *   `PushBack` with `send_state: Queued`, then `Sending`, then `Sent` with its local message id;
 * - the Telegram application pair starts configured (as if it had been entered in Settings →
 *   Networks), so `ClearTelegramApp` is the quickest way to walk the setup form;
 * - room-list and timeline changes are emitted as `VectorDiffDto` operations against what each
 *   subscription has already received, never as full snapshots after the initial `Reset`.
 */

import type {
  Account,
  AccountId,
  CookieFieldDto,
  DeviceDto,
  EngineCommand,
  EngineEvent,
  EngineResponse,
  EventId,
  GlobalSearchHit,
  LoginId,
  LoginInputDto,
  LoginStepDto,
  Network,
  NetworkInfoDto,
  NetworkSettingDto,
  NetworkSettingKind,
  PairingPhase,
  RoomId,
  RoomMediaItem,
  RoomMediaKind,
  RoomMemberDto,
  RoomSearchHit,
  RoomSummary,
  RoomTagDef,
  SyncState,
  SyncStatusReport,
  TimelineItemDto,
  UserId,
  VectorDiffDto,
} from '../generated/index.js';
import type { EventHandler, Transport, Unsubscribe } from './transport.js';
import {
  capabilitiesFor,
  dayDivider,
  previewOf,
  seedDemo,
  sortRoomList,
  stableId,
  svgImageDataUrl,
  textFileDataUrl,
  wavDataUrl,
} from '../mock/seed.js';

/** The only login code the demo accepts, matching the fake connector's `FAKE_CODE`. */
export const MOCK_LOGIN_CODE = '12345';

/**
 * A stable demonstration pin, derived from nothing real: the dev walk only needs a fingerprint
 * to display, compare and carry through a phrase recovery.
 */
const DEMO_SYNC_PIN = 'a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0';

/** Messenger cookies the demo asks for; `datr` is optional, the other two are not. */
const MESSENGER_COOKIES: CookieFieldDto[] = [
  { name: 'c_user', label: 'c_user (the numeric id in your Facebook profile URL)', required: true },
  { name: 'xs', label: 'xs (the session cookie)', required: true },
  { name: 'datr', label: 'datr (the browser cookie)', required: false },
];

/** One Messenger runtime knob, as `ListNetworks` declares it. */
interface MockMessengerSetting {
  key: string;
  label: string;
  kind: NetworkSettingKind;
  options: { value: string; label: string }[];
  /** Value in effect when nothing is stored; clearing restores it. */
  default: string | null;
}

/** Messenger's runtime knobs, mirroring the engine's declaration in `factory.rs`. */
const MESSENGER_SETTINGS: MockMessengerSetting[] = [
  {
    key: 'proxy',
    label: 'Proxy URL (http://, https://, socks5://)',
    kind: 'text',
    options: [],
    default: null,
  },
  {
    key: 'platform',
    label: 'Platform',
    kind: 'choice',
    options: [
      { value: 'android', label: 'Messenger for Android' },
      { value: 'ios', label: 'Messenger for iOS' },
    ],
    default: 'android',
  },
];

/** Where the Telegram application pair is registered; `ListNetworks` hands it to the UI. */
const TELEGRAM_APPS_URL = 'https://my.telegram.org/apps';

/** The engine's `not_supported` reason when Telegram has no application pair yet. */
const TELEGRAM_APP_FEATURE = 'telegram application credentials';

/** The pair the demo starts with, as if entered in Settings → Networks on this device. */
const DEMO_TELEGRAM_APP = {
  api_id: 2040,
  api_hash: '1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d',
};

const THEMES: [string, string][] = [
  ['dark', 'Dark'],
  ['light', 'Light'],
  ['amoled', 'AMOLED'],
];

/** A demo upload has no real file behind it; progress and the row still need a believable size. */
const DEMO_UPLOAD_BYTES = 1_842_176;

interface RoomSubscription {
  filter: string | null;
  /** The rows this subscription has received; diffs are computed against it. */
  rows: RoomSummary[];
  /** False until the initial `Reset` went out, so early changes do not double-apply. */
  ready: boolean;
}

interface LoginSession {
  id: LoginId;
  network: Network;
  step: LoginStepDto;
  phone: string | null;
  cookies: Record<string, string>;
}

/** An engine failure the UI can branch on: `code` is coarse, `reason` is the stable detail. */
export class MockEngineError extends Error {
  readonly code: string;
  readonly reason: string | null;

  constructor(code: string, message: string, reason: string | null = null) {
    super(message);
    this.name = 'MockEngineError';
    this.code = code;
    this.reason = reason;
  }
}

function invalidArgument(message: string): MockEngineError {
  return new MockEngineError('invalid_argument', message);
}

function notSupported(message: string, reason: string): MockEngineError {
  return new MockEngineError('not_supported', message, reason);
}

function notFound(message: string): MockEngineError {
  return new MockEngineError('not_found', message);
}

const NETWORK_LABEL: Record<Network, string> = {
  telegram: 'Telegram',
  messenger: 'Messenger',
  fake: 'Test',
};

/** Mime type a picked file would carry, by extension, when the caller does not say. */
const MIME_BY_EXTENSION: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  svg: 'image/svg+xml',
  mp4: 'video/mp4',
  webm: 'video/webm',
  mp3: 'audio/mpeg',
  ogg: 'audio/ogg',
  opus: 'audio/ogg',
  wav: 'audio/wav',
  pdf: 'application/pdf',
  zip: 'application/zip',
};

/** Slug the demo tag catalogue assigns; the engine derives ids the same way. */
function tagSlug(name: string, taken: ReadonlySet<string>): string {
  const base =
    name
      .normalize('NFD')
      .replace(/[\u0300-\u036f]/g, '')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'tag';
  if (!taken.has(base)) return base;
  let suffix = 2;
  while (taken.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

/**
 * In-memory implementation of the v2 engine, seeded with a Telegram and a Messenger account.
 */
export class MockTransport implements Transport {
  private handlers = new Set<EventHandler>();

  private accounts: Account[];
  private rooms: RoomSummary[];
  private members: Map<RoomId, RoomMemberDto[]>;
  private timelines: Map<RoomId, TimelineItemDto[]>;
  private media: Map<string, string>;
  private notes: Map<RoomId, string>;
  private drafts: Map<RoomId, { text: string; reply_to: EventId | null }>;
  private tags: RoomTagDef[];
  private people: Record<UserId, string>;

  private roomSubscriptions = new Map<string, RoomSubscription>();
  private timelineSubscriptions = new Map<string, RoomId>();
  private logins = new Map<LoginId, LoginSession>();
  private settings = new Map<string, string>([
    ['notificationLevel', JSON.stringify('all')],
    ['compactRows', JSON.stringify(false)],
  ]);
  /** The Telegram application pair in effect; `null` until one is stored. */
  private telegramApp: { api_id: number; api_hash: string } | null = { ...DEMO_TELEGRAM_APP };
  /** Messenger's stored runtime settings; a key that was cleared falls back to its default. */
  private messengerSettings = new Map<string, string>();
  /** Live pairings seeded by `BeginPairing`/`AcceptPairingTicket`, so the dev UI can walk the states. */
  private pairings = new Map<
    string,
    { state: PairingPhase; sas: string | null; polls: number }
  >();
  /** The local device roster: this device, plus any device the dev walk pairs. */
  private devices: DeviceDto[] = [
    {
      device_id: 'demo-device-this-one',
      label: 'This device',
      enrolled_ms: Date.now(),
      epoch: 0,
      current: true,
    },
  ];
  private epoch = 0;
  /** The optional sync server, as configured through the dev walk. */
  private sync: {
    mode: 'off' | 'prefs';
    url: string | null;
    pin: string | null;
    tokenConfigured: boolean;
    state: SyncState;
    lastSyncMs: number | null;
    lastError: string | null;
    /** Set by a revoke and cleared by `CreateRecoveryPhrase`, as the engine reports it. */
    recoveryStale: boolean;
  } = {
    mode: 'off',
    url: null,
    pin: null,
    tokenConfigured: false,
    state: 'idle',
    lastSyncMs: null,
    lastError: null,
    recoveryStale: false,
  };
  private counter = 0;
  private txnCounter = 0;
  private announced = false;

  constructor() {
    const seed = seedDemo();
    this.accounts = seed.accounts;
    this.rooms = seed.rooms;
    this.members = seed.members;
    this.timelines = seed.timelines;
    this.media = seed.media;
    this.notes = seed.notes;
    this.drafts = seed.drafts;
    this.tags = seed.tags;
    this.people = seed.people;
  }

  // === Transport surface ===

  subscribe(handler: EventHandler): Unsubscribe {
    this.handlers.add(handler);
    if (!this.announced) {
      this.announced = true;
      this.announceConnection();
    }
    return () => {
      this.handlers.delete(handler);
    };
  }

  emit(event: EngineEvent): void {
    for (const handler of this.handlers) {
      try {
        handler(event);
      } catch (err) {
        console.error('[MockTransport] Error in subscriber:', err);
      }
    }
  }

  // === State helpers ===

  private accountList(): Account[] {
    return this.accounts.map((account) => ({ ...account, capabilities: { ...account.capabilities } }));
  }

  private accountById(accountId: AccountId): Account {
    const account = this.accounts.find((a) => a.id === accountId);
    if (!account) throw notFound(`no account ${accountId}`);
    return account;
  }

  private roomById(roomId: RoomId): RoomSummary {
    const room = this.rooms.find((r) => r.id === roomId);
    if (!room) throw notFound(`no chat ${roomId}`);
    return room;
  }

  private items(roomId: RoomId): TimelineItemDto[] {
    this.roomById(roomId);
    const items = this.timelines.get(roomId);
    if (!items) {
      const empty: TimelineItemDto[] = [];
      this.timelines.set(roomId, empty);
      return empty;
    }
    return items;
  }

  private myUserId(roomId: RoomId): UserId {
    const room = this.roomById(roomId);
    return this.accountById(room.account_id).remote_user_id ?? '';
  }

  private myDisplayName(roomId: RoomId): string | null {
    const room = this.roomById(roomId);
    return this.accountById(room.account_id).display_name;
  }

  private itemByEvent(roomId: RoomId, eventId: EventId): TimelineItemDto | null {
    return this.items(roomId).find((item) => item.event_id === eventId) ?? null;
  }

  /** Applies a chat-flag or preview change and lets every room-list subscription see it. */
  private touchRoom(roomId: RoomId, patch: Partial<RoomSummary>): void {
    const index = this.rooms.findIndex((room) => room.id === roomId);
    if (index === -1) throw notFound(`no chat ${roomId}`);
    this.rooms[index] = { ...this.rooms[index], ...patch };
    sortRoomList(this.rooms);
    this.syncRoomSubscriptions();
  }

  /** The body `SyncStatus` and `SyncConfigured` answer with, from the mock's own state. */
  private syncReport(): SyncStatusReport {
    const enabled = this.sync.mode === 'prefs';
    const updated = this.sync.lastSyncMs ?? 0;
    return {
      mode: this.sync.mode,
      url: this.sync.url,
      pin: this.sync.pin,
      token_configured: this.sync.tokenConfigured,
      state: this.sync.state,
      last_sync_ms: this.sync.lastSyncMs,
      last_error: this.sync.lastError,
      recovery_stale: this.sync.recoveryStale,
      layers: ['settings', 'tags', 'catalogue', 'backgrounds', 'theme', 'messages'].map((key) => ({
        key,
        // The message mirror is opt-in and not part of this slice, so it stays off.
        enabled: enabled && key !== 'messages',
        updated_at_ms: updated,
      })),
      devices: this.devices,
    };
  }

  private roomProjection(filter: string | null): RoomSummary[] {
    const needle = (filter ?? '').trim().toLowerCase();
    if (needle === '') return [...this.rooms];
    return this.rooms.filter((room) => (room.name ?? '').toLowerCase().includes(needle));
  }

  private syncRoomSubscriptions(): void {
    for (const [subscriptionId, subscription] of this.roomSubscriptions) {
      if (!subscription.ready) continue;
      const next = this.roomProjection(subscription.filter);
      this.emitRoomDiff(subscriptionId, next, subscription.rows);
      subscription.rows = next;
    }
  }

  /** Emits the smallest set of operations that turns `prev` into `next`, in order. */
  private emitRoomDiff(subscriptionId: string, next: RoomSummary[], prev: RoomSummary[]): void {
    const nextIds = new Set(next.map((room) => room.id));
    for (let index = prev.length - 1; index >= 0; index -= 1) {
      if (!nextIds.has(prev[index].id)) {
        this.emit({
          type: 'RoomListDiff',
          payload: { subscription_id: subscriptionId, diff: { type: 'Remove', content: { index } } },
        });
      }
    }
    const current = prev.filter((room) => nextIds.has(room.id));
    for (let index = 0; index < next.length; index += 1) {
      const row = next[index];
      const at = current.findIndex((room) => room.id === row.id);
      if (at === -1) {
        this.emit({
          type: 'RoomListDiff',
          payload: { subscription_id: subscriptionId, diff: { type: 'Insert', content: { index, value: row } } },
        });
        current.splice(index, 0, row);
      } else if (at !== index) {
        this.emit({
          type: 'RoomListDiff',
          payload: { subscription_id: subscriptionId, diff: { type: 'Remove', content: { index: at } } },
        });
        current.splice(at, 1);
        this.emit({
          type: 'RoomListDiff',
          payload: { subscription_id: subscriptionId, diff: { type: 'Insert', content: { index, value: row } } },
        });
        current.splice(index, 0, row);
      } else if (current[index] !== row) {
        this.emit({
          type: 'RoomListDiff',
          payload: { subscription_id: subscriptionId, diff: { type: 'Set', content: { index, value: row } } },
        });
        current[index] = row;
      }
    }
  }

  private emitTimelineDiff(roomId: RoomId, diff: VectorDiffDto<TimelineItemDto>): void {
    for (const [subscriptionId, subscribedRoom] of this.timelineSubscriptions) {
      if (subscribedRoom !== roomId) continue;
      this.emit({
        type: 'TimelineDiff',
        payload: { subscription_id: subscriptionId, room_id: roomId, diff },
      });
    }
  }

  /** Pushes a new message, inserting a date separator when it starts a new day. */
  private pushMessage(roomId: RoomId, item: TimelineItemDto): void {
    const items = this.items(roomId);
    const lastMessage = [...items].reverse().find((entry) => entry.content.kind !== 'DayDivider');
    const sameDay =
      lastMessage !== undefined &&
      new Date(lastMessage.timestamp_millis).toDateString() ===
        new Date(item.timestamp_millis).toDateString();
    if (!sameDay) {
      const divider = dayDivider(item.timestamp_millis);
      const known = items.some(
        (entry) => entry.content.kind === 'DayDivider' && entry.timestamp_millis === divider.timestamp_millis
      );
      if (!known) {
        items.push(divider);
        this.emitTimelineDiff(roomId, { type: 'PushBack', content: { value: divider } });
      }
    }
    items.push(item);
    this.emitTimelineDiff(roomId, { type: 'PushBack', content: { value: item } });
    this.touchRoom(roomId, { latest_event: previewOf(item) });
  }

  /** Replaces one timeline item's send state (and id, on the sent transition) with a `Set` diff. */
  private settleItem(
    roomId: RoomId,
    internalId: string,
    sendState: TimelineItemDto['send_state'],
    eventId?: EventId
  ): void {
    const items = this.timelines.get(roomId);
    if (!items) return;
    const index = items.findIndex((item) => item.internal_id === internalId);
    if (index === -1) return;
    const updated: TimelineItemDto = {
      ...items[index],
      send_state: sendState,
      event_id: eventId ?? items[index].event_id,
    };
    items[index] = updated;
    this.emitTimelineDiff(roomId, { type: 'Set', content: { index, value: updated } });
  }

  /**
   * The outbox walk every outgoing message takes: queued echo, optional upload progress, sending,
   * then sent with its local message id.
   */
  private queueOutgoing(roomId: RoomId, item: TimelineItemDto, txnId: string, uploadBytes?: number): void {
    setTimeout(() => {
      if (!this.rooms.some((room) => room.id === roomId)) return;
      this.pushMessage(roomId, item);
      if (uploadBytes !== undefined) {
        const report = (sent: number): void => {
          this.emit({
            type: 'UploadProgress',
            payload: { room_id: roomId, txn_id: txnId, sent_bytes: sent, total_bytes: uploadBytes },
          });
        };
        report(0);
        setTimeout(() => report(Math.round(uploadBytes / 2)), 10);
        setTimeout(() => report(uploadBytes), 20);
      }
      setTimeout(() => {
        this.settleItem(roomId, item.internal_id, { status: 'Sending', data: { transaction_id: txnId } });
      }, 5);
      setTimeout(() => {
        const eventId = stableId('event', `${roomId}:${txnId}`);
        this.settleItem(roomId, item.internal_id, { status: 'Sent', data: { event_id: eventId } }, eventId);
      }, 30);
    }, 0);
  }

  private nextTxnId(): string {
    this.txnCounter += 1;
    return `txn_${Date.now().toString(36)}_${this.txnCounter}`;
  }

  private announceConnection(): void {
    setTimeout(() => {
      for (const account of this.accounts) {
        this.emit({ type: 'ConnectionStatus', payload: { account_id: account.id, status: 'Syncing' } });
      }
      setTimeout(() => {
        for (const account of this.accounts) {
          this.emit({ type: 'ConnectionStatus', payload: { account_id: account.id, status: 'Connected' } });
        }
      }, 20);
    }, 0);
  }

  // === Login state machine ===

  private firstStep(network: Network): LoginStepDto {
    switch (network) {
      case 'telegram':
      case 'fake':
        return { type: 'phone_number' };
      case 'messenger':
        return {
          type: 'cookies',
          fields: MESSENGER_COOKIES.map((field) => ({ ...field })),
          help_url: 'https://www.facebook.com/help/',
        };
    }
  }

  private expected(input: string): LoginStepDto {
    return {
      type: 'failed',
      reason: { type: 'invalid_input', field: null, message: `Enter ${input}.` },
    };
  }

  private invalid(field: string, message: string): LoginStepDto {
    return { type: 'failed', reason: { type: 'invalid_input', field, message } };
  }

  private finishLogin(session: LoginSession): LoginStepDto {
    const network = session.network;
    const remoteUserId =
      network === 'telegram'
        ? `tg:${(session.phone ?? '').replace(/\D/g, '') || '0'}`
        : network === 'messenger'
          ? `fb:${session.cookies.c_user ?? '0'}`
          : `fake:${this.counter}`;
    const account: Account = {
      id: stableId('account', `${network}:${++this.counter}`),
      network,
      remote_user_id: remoteUserId,
      display_name: 'Mara Vogel',
      avatar: null,
      state: 'active',
      capabilities: capabilitiesFor(network),
    };
    this.accounts.push(account);
    this.emit({ type: 'AccountsChanged', payload: { accounts: this.accountList() } });
    this.emit({
      type: 'ToastNotice',
      payload: { message: `${NETWORK_LABEL[network]} account connected`, level: 'info' },
    });
    this.logins.delete(session.id);
    return { type: 'done', account: { ...account, capabilities: { ...account.capabilities } } };
  }

  private advanceLogin(session: LoginSession, input: LoginInputDto): LoginStepDto {
    const step = session.step;
    switch (session.network) {
      case 'telegram':
      case 'fake': {
        if (step.type === 'phone_number') {
          if (input.type !== 'phone_number') return this.expected('your phone number');
          const phone = input.phone.trim();
          if (phone === '') return this.invalid('phone', 'Enter your phone number in international format.');
          session.phone = phone;
          return {
            type: 'code',
            via: session.network === 'fake' ? 'app' : 'sms',
            length: MOCK_LOGIN_CODE.length,
          };
        }
        if (step.type === 'code') {
          if (input.type !== 'code') return this.expected('the login code');
          if (input.code.trim() !== MOCK_LOGIN_CODE) {
            return this.invalid('code', 'That code is not the one the network sent.');
          }
          if (session.network === 'telegram' && (session.phone ?? '').endsWith('9')) {
            return { type: 'password', hint: 'Two-step verification is on for this account.' };
          }
          return this.finishLogin(session);
        }
        if (step.type === 'password') {
          if (input.type !== 'password') return this.expected('the two-step password');
          if (input.password.trim() === '') {
            return this.invalid('password', 'Enter the password for two-step verification.');
          }
          return this.finishLogin(session);
        }
        return { type: 'failed', reason: { type: 'not_supported' } };
      }
      case 'messenger': {
        if (step.type === 'cookies') {
          if (input.type !== 'cookies') return this.expected('the browser cookies');
          for (const field of step.fields) {
            const value = (input.values[field.name] ?? '').trim();
            if (field.required && value === '') {
              return this.invalid(field.name, `Cookie ${field.name} is required.`);
            }
          }
          session.cookies = { ...input.values };
          return this.finishLogin(session);
        }
        return { type: 'failed', reason: { type: 'not_supported' } };
      }
    }
  }

  // === Shared media ===

  private mediaItems(roomId: RoomId, kind: RoomMediaKind): RoomMediaItem[] {
    const out: RoomMediaItem[] = [];
    for (const item of this.items(roomId)) {
      if (item.event_id === null) continue;
      const base = {
        event_id: item.event_id,
        sender: item.sender,
        sender_display_name: item.sender_display_name,
        timestamp_millis: item.timestamp_millis,
      };
      if (item.content.kind === 'Media') {
        const data = item.content.data;
        const visual = data.media_kind === 'image' || data.media_kind === 'video' || data.media_kind === 'sticker';
        if ((kind === 'Media' && visual) || (kind === 'Files' && !visual)) {
          out.push({
            ...base,
            kind: data.media_kind,
            body: data.body,
            source: data.source,
            mime_type: data.mime_type,
            size_bytes: data.size_bytes,
            width: data.width,
            height: data.height,
          });
        }
      } else if (item.content.kind === 'TextMessage' && kind === 'Links') {
        const urls = item.content.data.body.match(/https?:\/\/[^\s<>"]+/g) ?? [];
        if (urls.length > 0) {
          out.push({
            ...base,
            kind: 'link',
            body: item.content.data.body,
            source: null,
            mime_type: null,
            size_bytes: null,
            width: null,
            height: null,
            urls,
          });
        }
      }
    }
    return out.reverse();
  }

  /**
   * `ListNetworks`: Telegram is always offered, ready only once its pair is stored; Messenger
   * always declares its runtime settings, with the values stored here.
   */
  private networkInfo(): NetworkInfoDto[] {
    const configured = this.telegramApp !== null;
    return [
      {
        network: 'telegram',
        ready: configured,
        setup: configured
          ? null
          : {
              kind: 'telegram_app',
              app_url: TELEGRAM_APPS_URL,
              credentials: [
                { key: 'api_id', label: 'API ID', secret: false },
                { key: 'api_hash', label: 'API hash', secret: true },
              ],
              settings: [],
            },
      },
      {
        network: 'messenger',
        ready: true,
        setup: {
          kind: 'messenger',
          app_url: '',
          credentials: [],
          settings: MESSENGER_SETTINGS.map((spec) => this.settingDto(spec)),
        },
      },
      { network: 'fake', ready: true, setup: null },
    ];
  }

  /** One declared Messenger setting with the value in effect: stored, else its default. */
  private settingDto(spec: MockMessengerSetting): NetworkSettingDto {
    return {
      key: spec.key,
      label: spec.label,
      kind: spec.kind,
      options: spec.options.map((option) => ({ ...option })),
      value: this.messengerSettings.get(spec.key) ?? spec.default,
    };
  }

  /** The declaration of `key` on `network`; anything else is what the engine calls invalid. */
  private messengerSetting(network: Network, key: string): MockMessengerSetting {
    const spec =
      network === 'messenger' ? MESSENGER_SETTINGS.find((entry) => entry.key === key) : undefined;
    if (!spec) throw invalidArgument(`${network} does not declare a "${key}" setting`);
    return spec;
  }

  // === Command handling ===

  async invoke(command: EngineCommand): Promise<EngineResponse> {
    switch (command.action) {
      case 'ListAccounts':
        return { type: 'AccountsList', payload: { accounts: this.accountList() } };

      case 'ListNetworks':
        return { type: 'Networks', payload: { networks: this.networkInfo() } };

      case 'SetTelegramApp': {
        const { api_id, api_hash } = command.params;
        if (!Number.isInteger(api_id) || api_id <= 0) {
          throw invalidArgument('the Telegram api_id must be a positive number');
        }
        if (!/^[0-9a-fA-F]{32}$/.test(api_hash.trim())) {
          throw invalidArgument('the Telegram api_hash must be the 32-character hexadecimal value');
        }
        this.telegramApp = { api_id, api_hash: api_hash.trim() };
        return { type: 'TelegramAppSet', payload: { configured: true } };
      }

      case 'ClearTelegramApp':
        this.telegramApp = null;
        return { type: 'TelegramAppCleared' };

      case 'GetTelegramApp':
        return {
          type: 'TelegramApp',
          payload: {
            configured: this.telegramApp !== null,
            api_id: this.telegramApp?.api_id ?? null,
            source: this.telegramApp === null ? 'none' : 'runtime',
          },
        };

      case 'SetNetworkSetting': {
        const spec = this.messengerSetting(command.params.network, command.params.key);
        const value = command.params.value.trim();
        if (value === '') {
          // The engine treats an empty value as a clear; the declared default applies again.
          this.messengerSettings.delete(spec.key);
        } else if (spec.kind === 'choice') {
          if (!spec.options.some((option) => option.value === value)) {
            throw invalidArgument(
              `the ${spec.key} setting accepts only ${spec.options
                .map((option) => option.value)
                .join(', ')}`
            );
          }
          this.messengerSettings.set(spec.key, value);
        } else {
          if (spec.key === 'proxy' && !/^(https?|socks5):\/\/\S+$/i.test(value)) {
            throw invalidArgument(
              'the proxy must be an http://, https:// or socks5:// URL with a host'
            );
          }
          this.messengerSettings.set(spec.key, value);
        }
        return { type: 'NetworkSettingSet', payload: { setting: this.settingDto(spec) } };
      }

      case 'ClearNetworkSetting': {
        const spec = this.messengerSetting(command.params.network, command.params.key);
        this.messengerSettings.delete(spec.key);
        return { type: 'NetworkSettingCleared' };
      }

      case 'BeginLogin': {
        const info = this.networkInfo().find(
          (candidate) => candidate.network === command.params.network
        );
        if (!info?.ready) {
          throw notSupported(
            'Telegram needs its application pair first: enter the api_id and api_hash from ' +
              `${TELEGRAM_APPS_URL} in Settings > Networks`,
            TELEGRAM_APP_FEATURE
          );
        }
        const loginId = `login_${++this.counter}`;
        const step = this.firstStep(command.params.network);
        this.logins.set(loginId, {
          id: loginId,
          network: command.params.network,
          step,
          phone: null,
          cookies: {},
        });
        return { type: 'LoginStep', payload: { login_id: loginId, step } };
      }

      case 'SubmitLogin': {
        const session = this.logins.get(command.params.login_id);
        if (!session) throw notFound('this login attempt has ended; start again');
        const step = this.advanceLogin(session, command.params.input);
        session.step = step;
        return { type: 'LoginStep', payload: { login_id: session.id, step } };
      }

      case 'CancelLogin':
        this.logins.delete(command.params.login_id);
        return { type: 'LoginCancelled' };

      case 'Logout': {
        const accountId = command.params.account_id;
        this.accountById(accountId);
        const removed = this.rooms.filter((room) => room.account_id === accountId).map((room) => room.id);
        this.accounts = this.accounts.filter((account) => account.id !== accountId);
        this.rooms = this.rooms.filter((room) => room.account_id !== accountId);
        for (const roomId of removed) {
          this.timelines.delete(roomId);
          this.members.delete(roomId);
          this.notes.delete(roomId);
          this.drafts.delete(roomId);
          for (const [subscriptionId, subscribedRoom] of this.timelineSubscriptions) {
            if (subscribedRoom === roomId) this.timelineSubscriptions.delete(subscriptionId);
          }
        }
        this.emit({ type: 'AccountsChanged', payload: { accounts: this.accountList() } });
        this.syncRoomSubscriptions();
        return { type: 'LoggedOut', payload: { account_id: accountId } };
      }

      case 'ReconnectAccount': {
        // A reconnect only rebuilds the network session: the account and its history stay. The
        // demo engine announces the round trip so the banner can show it happened.
        const accountId = command.params.account_id;
        this.accountById(accountId);
        this.emit({
          type: 'ConnectionStatus',
          payload: { account_id: accountId, status: 'Connecting' },
        });
        queueMicrotask(() =>
          this.emit({
            type: 'ConnectionStatus',
            payload: { account_id: accountId, status: 'Connected' },
          }),
        );
        return { type: 'AccountReconnected' };
      }

      case 'SubscribeRoomList': {
        const subscriptionId = `sub_rooms_${++this.counter}`;
        this.roomSubscriptions.set(subscriptionId, { filter: command.params.filter, rows: [], ready: false });
        setTimeout(() => {
          const subscription = this.roomSubscriptions.get(subscriptionId);
          if (!subscription) return;
          subscription.rows = this.roomProjection(subscription.filter);
          subscription.ready = true;
          this.emit({
            type: 'RoomListDiff',
            payload: {
              subscription_id: subscriptionId,
              diff: { type: 'Reset', content: { values: [...subscription.rows] } },
            },
          });
        }, 0);
        return { type: 'RoomListSubscribed', payload: { subscription_id: subscriptionId } };
      }

      case 'Unsubscribe':
        this.roomSubscriptions.delete(command.params.subscription_id);
        this.timelineSubscriptions.delete(command.params.subscription_id);
        return { type: 'Unsubscribed' };

      case 'GetRoomMembers': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        const members = this.members.get(roomId) ?? [];
        return { type: 'RoomMembers', payload: { members: members.map((member) => ({ ...member })) } };
      }

      case 'GetRoomDetails': {
        const roomId = command.params.room_id;
        const room = this.roomById(roomId);
        const account = this.accountById(room.account_id);
        return {
          type: 'RoomDetailsRetrieved',
          payload: {
            details: {
              room_id: room.id,
              account_id: room.account_id,
              network: room.network,
              name: room.name,
              avatar: room.avatar,
              kind: room.kind,
              is_direct: room.is_direct,
              member_count: (this.members.get(roomId) ?? []).length,
              note: this.notes.get(roomId) ?? null,
              capabilities: { ...account.capabilities },
            },
          },
        };
      }

      case 'GetUserProfile': {
        const account = this.accountById(command.params.account_id);
        const userId = command.params.user_id;
        let displayName = this.people[userId] ?? null;
        let dmRoomId: RoomId | null = null;
        for (const room of this.rooms) {
          if (room.account_id !== account.id) continue;
          const member = (this.members.get(room.id) ?? []).find((entry) => entry.user_id === userId);
          if (member) displayName = member.display_name ?? displayName;
          if (room.is_direct && member) dmRoomId = room.id;
        }
        return {
          type: 'UserProfile',
          payload: {
            profile: {
              user_id: userId,
              account_id: account.id,
              display_name: displayName,
              avatar: null,
              is_me: account.remote_user_id === userId,
              dm_room_id: dmRoomId,
            },
          },
        };
      }

      case 'GetRoomNote':
        this.roomById(command.params.room_id);
        return { type: 'RoomNote', payload: { note: this.notes.get(command.params.room_id) ?? null } };

      case 'SetRoomNote': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        if (command.params.note.trim() === '') this.notes.delete(roomId);
        else this.notes.set(roomId, command.params.note);
        return { type: 'RoomNoteSet' };
      }

      case 'GetTagCatalog':
        return { type: 'TagCatalog', payload: { tags: this.tags.map((tag) => ({ ...tag })) } };

      case 'CreateRoomTag': {
        const name = command.params.name.trim();
        if (name === '') throw invalidArgument('a tag needs a name');
        const tag: RoomTagDef = {
          id: tagSlug(name, new Set(this.tags.map((entry) => entry.id))),
          name,
          colour: command.params.colour,
        };
        this.tags = [...this.tags, tag];
        return { type: 'RoomTagCreated', payload: { tag: { ...tag } } };
      }

      case 'RenameRoomTag': {
        const index = this.tags.findIndex((tag) => tag.id === command.params.id);
        if (index === -1) throw notFound(`no tag ${command.params.id}`);
        if (command.params.name === null && command.params.colour === null) {
          throw invalidArgument('a rename needs a name or a colour');
        }
        const current = this.tags[index];
        const next: RoomTagDef = {
          ...current,
          name: command.params.name?.trim() || current.name,
          colour: command.params.colour ?? current.colour,
        };
        this.tags = this.tags.map((tag, at) => (at === index ? next : tag));
        return { type: 'RoomTagsUpdated' };
      }

      case 'ReorderRoomTags': {
        const ids = command.params.ids;
        const known = new Set(this.tags.map((tag) => tag.id));
        const permutation =
          ids.length === known.size &&
          new Set(ids).size === ids.length &&
          ids.every((id) => known.has(id));
        if (!permutation) throw invalidArgument('ids must list the catalogue exactly once');
        this.tags = ids.map((id) => this.tags.find((tag) => tag.id === id) as RoomTagDef);
        return { type: 'RoomTagsUpdated' };
      }

      case 'SetRoomTag': {
        const room = this.roomById(command.params.room_id);
        if (!this.tags.some((tag) => tag.id === command.params.tag_id)) {
          throw notFound(`no tag ${command.params.tag_id}`);
        }
        const tags = new Set(room.tags);
        if (command.params.enabled) tags.add(command.params.tag_id);
        else tags.delete(command.params.tag_id);
        this.touchRoom(room.id, { tags: [...tags].sort() });
        return { type: 'RoomFlagsUpdated' };
      }

      case 'DeleteRoomTag': {
        const tagId = command.params.tag_id;
        if (!this.tags.some((tag) => tag.id === tagId)) throw notFound(`no tag ${tagId}`);
        this.tags = this.tags.filter((tag) => tag.id !== tagId);
        for (const room of this.rooms.filter((entry) => entry.tags.includes(tagId))) {
          this.touchRoom(room.id, { tags: room.tags.filter((id) => id !== tagId) });
        }
        return { type: 'RoomTagsUpdated' };
      }

      case 'MarkRoomRead':
        this.roomById(command.params.room_id);
        this.touchRoom(command.params.room_id, {
          unread_notifications: { highlight_count: 0, notification_count: 0 },
        });
        return { type: 'RoomFlagsUpdated' };

      case 'SetRoomPinned': {
        const room = this.roomById(command.params.room_id);
        // Mirrors the engine: a new pin goes to 0 and pushes the account's other pins down;
        // unpinning clears the order and closes the gap it leaves. Renumber the block in one pass.
        const others = this.rooms
          .filter((r) => r.account_id === room.account_id && r.pinned && r.id !== room.id)
          .sort((a, b) => (a.pin_order ?? Number.MAX_SAFE_INTEGER) - (b.pin_order ?? Number.MAX_SAFE_INTEGER));
        if (command.params.pinned) {
          this.touchRoom(room.id, { pinned: true, pin_order: 0 });
          for (const [index, other] of others.entries()) {
            this.touchRoom(other.id, { pin_order: index + 1 });
          }
        } else {
          this.touchRoom(room.id, { pinned: false, pin_order: null });
          for (const [index, other] of others.entries()) {
            this.touchRoom(other.id, { pin_order: index });
          }
        }
        return { type: 'RoomFlagsUpdated' };
      }

      case 'ReorderPinnedRooms': {
        const accountId = command.params.account_id;
        this.accountById(accountId);
        const ids = command.params.room_ids;
        const block = this.rooms.filter((r) => r.account_id === accountId && r.pinned).map((r) => r.id);
        const permutation = ids.length === block.length && block.every((id) => ids.includes(id));
        if (!permutation) throw invalidArgument('room_ids must list the pinned chats of the account exactly once');
        for (const [index, id] of ids.entries()) this.touchRoom(id, { pin_order: index });
        return { type: 'PinnedRoomsReordered', payload: { room_ids: [...ids] } };
      }

      case 'GetPinnedMessages': {
        const items = this.items(command.params.room_id).filter((item) => item.pinned && item.event_id !== null);
        return { type: 'PinnedMessages', payload: { items } };
      }

      case 'SetMessagePinned': {
        const roomId = command.params.room_id;
        const items = this.items(roomId);
        const index = items.findIndex((item) => item.event_id === command.params.event_id);
        if (index === -1) throw notFound(`no message ${command.params.event_id}`);
        const updated: TimelineItemDto = { ...items[index], pinned: command.params.pinned };
        items[index] = updated;
        this.emitTimelineDiff(roomId, { type: 'Set', content: { index, value: updated } });
        return { type: 'MessagePinSet', payload: { pinned: command.params.pinned } };
      }

      case 'SetRoomArchived':
        this.roomById(command.params.room_id);
        this.touchRoom(command.params.room_id, { archived: command.params.archived });
        return { type: 'RoomFlagsUpdated' };

      case 'SetRoomMarkedUnread':
        this.roomById(command.params.room_id);
        this.touchRoom(command.params.room_id, { marked_unread: command.params.unread });
        return { type: 'RoomFlagsUpdated' };

      case 'SetRoomMuted':
        this.roomById(command.params.room_id);
        this.touchRoom(command.params.room_id, { muted_until: command.params.muted_until });
        return { type: 'RoomFlagsUpdated' };

      case 'GetRoomMedia': {
        const roomId = command.params.room_id;
        const limit = command.params.limit;
        if (limit === 0) throw invalidArgument('limit must be at least one');
        const all = this.mediaItems(roomId, command.params.kind);
        const offset = command.params.before === null ? 0 : Number.parseInt(command.params.before, 10) || 0;
        const page = all.slice(offset, offset + limit);
        const end = offset + page.length;
        return {
          type: 'RoomMedia',
          payload: { items: page, next: end < all.length ? String(end) : null },
        };
      }

      case 'SearchRoomMessages': {
        const roomId = command.params.room_id;
        const limit = command.params.limit;
        const needle = command.params.query.trim().toLowerCase();
        if (limit === 0) throw invalidArgument('limit must be at least one');
        if (needle === '') throw invalidArgument('search query must not be blank');
        const hits: RoomSearchHit[] = [];
        for (const item of this.items(roomId)) {
          if (item.event_id === null || item.content.kind !== 'TextMessage') continue;
          const body = item.content.data.body;
          if (!body.toLowerCase().includes(needle)) continue;
          hits.push({
            event_id: item.event_id,
            sender: item.sender,
            sender_display_name: item.sender_display_name,
            timestamp_millis: item.timestamp_millis,
            body,
          });
        }
        hits.reverse();
        return { type: 'RoomSearchResults', payload: { hits: hits.slice(0, limit) } };
      }

      case 'SearchAllMessages': {
        const needle = command.params.query.trim().toLowerCase();
        if (needle === '') throw invalidArgument('search query must not be blank');
        const limit = command.params.limit ?? 20;
        if (limit === 0) throw invalidArgument('limit must be at least one');
        const hits: GlobalSearchHit[] = [];
        for (const room of this.rooms) {
          for (const item of this.timelines.get(room.id) ?? []) {
            if (item.event_id === null || item.content.kind !== 'TextMessage') continue;
            const body = item.content.data.body;
            if (!body.toLowerCase().includes(needle)) continue;
            hits.push({
              room_id: room.id,
              room_name: room.name,
              event_id: item.event_id,
              sender: item.sender,
              sender_display_name: item.sender_display_name,
              timestamp_millis: item.timestamp_millis,
              body,
            });
          }
        }
        hits.sort((a, b) => b.timestamp_millis - a.timestamp_millis);
        return { type: 'AllMessagesFound', payload: { hits: hits.slice(0, limit) } };
      }

      case 'ClearMediaCache':
        return { type: 'MediaCacheCleared' };

      case 'RefreshSync': {
        for (const account of this.accounts) {
          this.emit({ type: 'ConnectionStatus', payload: { account_id: account.id, status: 'Connecting' } });
        }
        setTimeout(() => {
          for (const account of this.accounts) {
            this.emit({ type: 'ConnectionStatus', payload: { account_id: account.id, status: 'Connected' } });
          }
        }, 20);
        return { type: 'SyncRefreshed' };
      }

      case 'SubscribeTimeline': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        const subscriptionId = `sub_tl_${++this.counter}`;
        this.timelineSubscriptions.set(subscriptionId, roomId);
        setTimeout(() => {
          if (!this.timelineSubscriptions.has(subscriptionId)) return;
          this.emit({
            type: 'TimelineDiff',
            payload: {
              subscription_id: subscriptionId,
              room_id: roomId,
              diff: { type: 'Reset', content: { values: [...(this.timelines.get(roomId) ?? [])] } },
            },
          });
        }, 0);
        return { type: 'TimelineSubscribed', payload: { subscription_id: subscriptionId } };
      }

      case 'CloseTimeline': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        for (const [subscriptionId, subscribedRoom] of this.timelineSubscriptions) {
          if (subscribedRoom === roomId) this.timelineSubscriptions.delete(subscriptionId);
        }
        return { type: 'TimelineClosed' };
      }

      case 'PaginateBackwards':
        this.roomById(command.params.room_id);
        // The demo timelines hold their whole history already.
        return { type: 'Paginated', payload: { reached_start: true } };

      case 'SendText': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        const replyTo =
          command.params.reply_to === null ? null : this.itemByEvent(roomId, command.params.reply_to);
        if (command.params.reply_to !== null && replyTo === null) {
          throw notFound(`no message ${command.params.reply_to}`);
        }
        const txnId = command.params.txn_id ?? this.nextTxnId();
        const item: TimelineItemDto = {
          internal_id: `local_${txnId}`,
          event_id: null,
          sender: this.myUserId(roomId),
          sender_display_name: this.myDisplayName(roomId),
          sender_avatar: null,
          timestamp_millis: Date.now(),
          content: {
            kind: 'TextMessage',
            data: { body: command.params.body, formatted_body: null, html_sanitized: true },
          },
          send_state: { status: 'Queued', data: { transaction_id: txnId } },
          is_own: true,
          reply_to: replyTo,
          read_by: [],
          edited: false,
          reactions: [],
          pinned: false,
          transaction_id: txnId,
        };
        this.queueOutgoing(roomId, item, txnId);
        return { type: 'MessageQueued', payload: { txn_id: txnId } };
      }

      case 'EditMessage': {
        const roomId = command.params.room_id;
        const items = this.items(roomId);
        const index = items.findIndex((item) => item.event_id === command.params.event_id);
        if (index === -1) throw notFound(`no message ${command.params.event_id}`);
        const current = items[index];
        if (current.content.kind !== 'TextMessage') {
          throw invalidArgument('only text messages can be edited');
        }
        const updated: TimelineItemDto = {
          ...current,
          content: {
            kind: 'TextMessage',
            data: { body: command.params.body, formatted_body: null, html_sanitized: true },
          },
          edited: true,
        };
        items[index] = updated;
        this.emitTimelineDiff(roomId, { type: 'Set', content: { index, value: updated } });
        return { type: 'MessageEdited' };
      }

      case 'DeleteMessage': {
        const roomId = command.params.room_id;
        const items = this.items(roomId);
        const index = items.findIndex((item) => item.event_id === command.params.event_id);
        if (index === -1) throw notFound(`no message ${command.params.event_id}`);
        const updated: TimelineItemDto = { ...items[index], content: { kind: 'Deleted' } };
        items[index] = updated;
        this.emitTimelineDiff(roomId, { type: 'Set', content: { index, value: updated } });
        return { type: 'MessageDeleted' };
      }

      case 'ToggleReaction': {
        const roomId = command.params.room_id;
        const items = this.items(roomId);
        const index = items.findIndex((item) => item.event_id === command.params.event_id);
        if (index === -1) throw notFound(`no message ${command.params.event_id}`);
        const me = this.myUserId(roomId);
        const key = command.params.key;
        const current = items[index];
        const existing = current.reactions.find((reaction) => reaction.key === key);
        let reactions: TimelineItemDto['reactions'];
        if (existing && existing.by_me) {
          const senders = existing.senders.filter((sender) => sender !== me);
          const count = existing.count - 1;
          reactions = count <= 0
            ? current.reactions.filter((reaction) => reaction.key !== key)
            : current.reactions.map((reaction) =>
                reaction.key === key ? { ...reaction, count, senders, by_me: false } : reaction
              );
        } else if (existing) {
          reactions = current.reactions.map((reaction) =>
            reaction.key === key
              ? { ...reaction, count: reaction.count + 1, senders: [me, ...reaction.senders], by_me: true }
              : reaction
          );
        } else {
          reactions = [...current.reactions, { key, count: 1, senders: [me], by_me: true }];
        }
        const updated: TimelineItemDto = { ...current, reactions };
        items[index] = updated;
        this.emitTimelineDiff(roomId, { type: 'Set', content: { index, value: updated } });
        return { type: 'ReactionToggled' };
      }

      case 'ForwardEvents': {
        const roomId = command.params.room_id;
        const items = this.items(roomId);
        const selected = command.params.event_ids
          .map((eventId) => items.find((item) => item.event_id === eventId))
          .filter((item): item is TimelineItemDto => item !== undefined);
        if (selected.length === 0) throw invalidArgument('no messages to forward');
        const failures: { room_id: RoomId; error: string }[] = [];
        for (const target of command.params.targets) {
          if (!this.rooms.some((room) => room.id === target)) {
            failures.push({ room_id: target, error: 'chat not found' });
            continue;
          }
          for (const source of selected) {
            const txnId = this.nextTxnId();
            const item: TimelineItemDto = {
              internal_id: `local_${txnId}`,
              event_id: null,
              sender: this.myUserId(target),
              sender_display_name: this.myDisplayName(target),
              sender_avatar: null,
              timestamp_millis: Date.now(),
              content: source.content,
              send_state: { status: 'Queued', data: { transaction_id: txnId } },
              is_own: true,
              reply_to: null,
              read_by: [],
              edited: false,
              reactions: [],
              pinned: false,
              transaction_id: txnId,
            };
            this.queueOutgoing(target, item, txnId);
          }
        }
        return { type: 'EventsForwarded', payload: { failures } };
      }

      case 'SaveDraft': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        const { text, reply_to } = command.params;
        if (text.trim() === '' && reply_to === null) {
          this.drafts.delete(roomId);
        } else {
          this.drafts.set(roomId, { text, reply_to });
        }
        this.touchRoom(roomId, { draft: text.trim() === '' ? null : text.slice(0, 80) });
        return { type: 'DraftSaved' };
      }

      case 'GetDraft': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        const draft = this.drafts.get(roomId);
        return {
          type: 'Draft',
          payload: { draft: draft ? { text: draft.text, reply_to: draft.reply_to } : null },
        };
      }

      case 'ListDrafts': {
        const drafts = [...this.drafts.entries()]
          .filter(([, draft]) => draft.text.trim() !== '')
          .map(([roomId, draft]) => ({ room_id: roomId, text: draft.text.slice(0, 80) }));
        return { type: 'Drafts', payload: { drafts } };
      }

      case 'RetrySend': {
        const roomId = command.params.room_id;
        const items = this.items(roomId);
        const index = items.findIndex((item) => item.transaction_id === command.params.txn_id);
        if (index === -1) throw notFound(`no queued message ${command.params.txn_id}`);
        const current = items[index];
        if (current.send_state.status === 'Sent') {
          throw invalidArgument('this message was already sent');
        }
        const txnId = command.params.txn_id;
        setTimeout(() => {
          this.settleItem(roomId, current.internal_id, { status: 'Sending', data: { transaction_id: txnId } });
        }, 0);
        setTimeout(() => {
          const eventId = stableId('event', `${roomId}:${txnId}`);
          this.settleItem(roomId, current.internal_id, { status: 'Sent', data: { event_id: eventId } }, eventId);
        }, 20);
        return { type: 'SendRetried' };
      }

      case 'CancelSend': {
        const roomId = command.params.room_id;
        const items = this.items(roomId);
        const index = items.findIndex((item) => item.transaction_id === command.params.txn_id);
        if (index === -1) throw notFound(`no queued message ${command.params.txn_id}`);
        items.splice(index, 1);
        this.emitTimelineDiff(roomId, { type: 'Remove', content: { index } });
        const latest = [...items].reverse().map((item) => previewOf(item)).find((preview) => preview !== null);
        this.touchRoom(roomId, { latest_event: latest ?? null });
        return { type: 'SendCancelled' };
      }

      case 'SendAttachment': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        const txnId = command.params.txn_id ?? this.nextTxnId();
        const extension = command.params.file_path.slice(command.params.file_path.lastIndexOf('.') + 1).toLowerCase();
        const mimeType =
          command.params.mime_type ?? MIME_BY_EXTENSION[extension] ?? 'application/octet-stream';
        const mediaId = `local_${txnId}`;
        const fileName = command.params.file_path.split(/[\\/]/).pop() ?? 'attachment';
        const body = command.params.body ?? fileName;
        const kind = mimeType.startsWith('image/')
          ? 'image'
          : mimeType.startsWith('video/')
            ? 'video'
            : mimeType.startsWith('audio/')
              ? 'audio'
              : 'file';
        const dataUrl =
          kind === 'image' ? svgImageDataUrl(body, 1_200, 800) : textFileDataUrl(`Uploaded from ${fileName}`);
        this.media.set(mediaId, dataUrl);
        const item: TimelineItemDto = {
          internal_id: `local_${txnId}`,
          event_id: null,
          sender: this.myUserId(roomId),
          sender_display_name: this.myDisplayName(roomId),
          sender_avatar: null,
          timestamp_millis: Date.now(),
          content: {
            kind: 'Media',
            data: {
              source: { media_id: mediaId },
              mime_type: mimeType,
              size_bytes: DEMO_UPLOAD_BYTES,
              body,
              media_kind: kind,
              width: kind === 'image' ? 1_200 : null,
              height: kind === 'image' ? 800 : null,
              duration_ms: null,
              voice: false,
            },
          },
          send_state: { status: 'Queued', data: { transaction_id: txnId } },
          is_own: true,
          reply_to: null,
          read_by: [],
          edited: false,
          reactions: [],
          pinned: false,
          transaction_id: txnId,
        };
        this.queueOutgoing(roomId, item, txnId, DEMO_UPLOAD_BYTES);
        return { type: 'MessageQueued', payload: { txn_id: txnId } };
      }

      case 'SendVoice': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        const txnId = command.params.txn_id ?? this.nextTxnId();
        const mediaId = `local_${txnId}`;
        const dataUrl = wavDataUrl(command.params.duration_ms);
        this.media.set(mediaId, dataUrl);
        const item: TimelineItemDto = {
          internal_id: `local_${txnId}`,
          event_id: null,
          sender: this.myUserId(roomId),
          sender_display_name: this.myDisplayName(roomId),
          sender_avatar: null,
          timestamp_millis: Date.now(),
          content: {
            kind: 'Media',
            data: {
              source: { media_id: mediaId },
              mime_type: 'audio/wav',
              size_bytes: dataUrl.length,
              body: 'Voice message',
              media_kind: 'voice',
              width: null,
              height: null,
              duration_ms: command.params.duration_ms,
              voice: true,
            },
          },
          send_state: { status: 'Queued', data: { transaction_id: txnId } },
          is_own: true,
          reply_to: null,
          read_by: [],
          edited: false,
          reactions: [],
          pinned: false,
          transaction_id: txnId,
        };
        this.queueOutgoing(roomId, item, txnId, dataUrl.length);
        return { type: 'MessageQueued', payload: { txn_id: txnId } };
      }

      case 'MarkRead': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        this.touchRoom(roomId, {
          unread_notifications: { highlight_count: 0, notification_count: 0 },
        });
        return { type: 'ReadMarked' };
      }

      case 'SetTyping': {
        const roomId = command.params.room_id;
        this.roomById(roomId);
        if (command.params.typing) {
          // The other side of the chat notices and starts typing back, so the indicator has
          // something to show in the demo.
          const me = this.myUserId(roomId);
          const peer = (this.members.get(roomId) ?? []).find((member) => member.user_id !== me);
          if (peer) {
            setTimeout(() => {
              if (this.handlers.size === 0) return;
              this.emit({ type: 'TypingNotice', payload: { room_id: roomId, users: [peer.user_id] } });
            }, 1_200);
            setTimeout(() => {
              if (this.handlers.size === 0) return;
              this.emit({ type: 'TypingNotice', payload: { room_id: roomId, users: [] } });
            }, 4_000);
          }
        }
        return { type: 'Typing' };
      }

      case 'GetMedia': {
        const dataUrl = this.media.get(command.params.source.media_id);
        if (dataUrl === undefined) throw notFound(`no media ${command.params.source.media_id}`);
        return { type: 'Media', payload: { path: dataUrl } };
      }

      case 'ListThemes':
        return {
          type: 'ThemesList',
          payload: { themes: THEMES.map(([id, name]): [string, string] => [id, name]) },
        };

      case 'GetTheme': {
        const theme = THEMES.find(([id]) => id === command.params.id);
        if (!theme) throw notFound(`no theme ${command.params.id}`);
        return {
          type: 'ThemeCss',
          payload: { css: `:root { --alloy-theme-id: "${theme[0]}"; }` },
        };
      }

      case 'SetActiveTheme': {
        const theme = THEMES.find(([id]) => id === command.params.id);
        if (!theme) throw notFound(`no theme ${command.params.id}`);
        return { type: 'ThemeSetActive' };
      }

      case 'GetSettings': {
        // Values are stored as the JSON text the caller sent, so the blob keeps their types.
        const values: Record<string, unknown> = {};
        for (const [key, json] of this.settings) values[key] = JSON.parse(json);
        return {
          type: 'SettingsRetrieved',
          payload: { settings_json: JSON.stringify(values) },
        };
      }

      case 'SetSetting': {
        try {
          JSON.parse(command.params.value_json);
        } catch {
          throw invalidArgument('value_json must be valid JSON');
        }
        this.settings.set(command.params.key, command.params.value_json);
        return { type: 'SettingSet', payload: { key: command.params.key } };
      }

      case 'ExportConfigBundle': {
        const bundle = JSON.stringify({
          schema: 1,
          exported_at_ms: Date.now(),
          device_label: 'This device',
          settings: Object.fromEntries(
            [...this.settings].map(([key, json]) => [key, JSON.parse(json) as unknown]),
          ),
          tags: [],
          catalogue: this.tags,
          backgrounds: [],
          theme: null,
        });
        return {
          type: 'ConfigBundleExported',
          payload: {
            bundle_json: bundle,
            bytes: bundle.length,
            summary: {
              settings: true,
              catalogue: this.tags.length > 0,
              tags: 0,
              backgrounds: command.params.include_backgrounds ? 0 : 0,
              theme: null,
            },
          },
        };
      }

      case 'ImportConfigBundle': {
        let parsed: { schema?: unknown };
        try {
          parsed = JSON.parse(command.params.bundle_json) as { schema?: unknown };
        } catch {
          throw invalidArgument('bundle_json must be valid JSON');
        }
        if (parsed.schema !== 1) {
          throw new MockEngineError(
            'bundle_rejected',
            `bundle schema ${String(parsed.schema)} is not supported by this build (1)`,
            `schema:${String(parsed.schema)}`,
          );
        }
        return {
          type: 'ConfigBundleImported',
          payload: {
            report: { settings: false, catalogue: false, tags: 0, backgrounds: 0, theme: false },
            applied: command.params.apply,
          },
        };
      }

      case 'BeginPairing': {
        if (command.params.role === 'accept') {
          throw invalidArgument('the accepting side starts with AcceptPairingTicket');
        }
        const id = `pair-${++this.counter}`;
        this.pairings.set(id, { state: 'waiting', sas: null, polls: 0 });
        this.emit({
          type: 'PairingChanged',
          payload: { pairing_id: id, state: 'waiting', sas: null },
        });
        return {
          type: 'PairingStarted',
          payload: {
            pairing_id: id,
            ticket: `alloy-pairing-v1|${id}|127.0.0.1:54100|00|`,
            code: 'AB34CD56',
            endpoint: '127.0.0.1:54100',
            expires_at_ms: Date.now() + 90_000,
          },
        };
      }

      case 'AcceptPairingTicket': {
        const id = `pair-${++this.counter}`;
        this.pairings.set(id, { state: 'peer_found', sas: null, polls: 0 });
        this.emit({
          type: 'PairingChanged',
          payload: { pairing_id: id, state: 'peer_found', sas: null },
        });
        return {
          type: 'PairingStarted',
          payload: {
            pairing_id: id,
            ticket: command.params.ticket,
            code: command.params.code,
            endpoint: '127.0.0.1:0',
            expires_at_ms: Date.now() + 90_000,
          },
        };
      }

      case 'PairingStatus': {
        const live = this.pairings.get(command.params.pairing_id);
        if (!live) throw notFound(`no pairing ${command.params.pairing_id}`);
        // There is no second device in the dev walk: after a few polls the handshake is reported
        // as complete so the SAS comparison is reachable. The event carries the same transition.
        live.polls += 1;
        if ((live.state === 'waiting' || live.state === 'peer_found') && live.polls >= 3) {
          live.state = 'sas_ready';
          live.sas = '497092';
          this.emit({
            type: 'PairingChanged',
            payload: {
              pairing_id: command.params.pairing_id,
              state: 'sas_ready',
              sas: live.sas,
            },
          });
        }
        return {
          type: 'PairingState',
          payload: {
            state: live.state,
            sas: live.sas,
            peer_label: null,
            transferred: 0,
            error: null,
          },
        };
      }

      case 'ConfirmPairing': {
        const live = this.pairings.get(command.params.pairing_id);
        if (!live) throw notFound(`no pairing ${command.params.pairing_id}`);
        // The real engine only reaches sas_ready after the handshake; the dev walk shows the
        // digits immediately so the comparison step is reachable without a second device.
        live.state = command.params.accept ? 'done' : 'declined';
        live.sas = command.params.accept ? live.sas ?? '497092' : live.sas;
        if (command.params.accept) {
          // A completed pairing enrols the other device, so the revoke path is walkable.
          const deviceId = `demo-device-paired-${command.params.pairing_id}`;
          if (!this.devices.some((device) => device.device_id === deviceId)) {
            this.devices = [
              ...this.devices,
              {
                device_id: deviceId,
                label: 'Paired phone',
                enrolled_ms: Date.now(),
                epoch: this.epoch,
                current: false,
              },
            ];
          }
        }
        this.emit({
          type: 'PairingChanged',
          payload: {
            pairing_id: command.params.pairing_id,
            state: live.state,
            sas: live.sas,
          },
        });
        return {
          type: 'PairingState',
          payload: {
            state: live.state,
            sas: live.sas,
            peer_label: null,
            transferred: command.params.accept ? 1024 : 0,
            error: null,
          },
        };
      }

      case 'CreateRecoveryPhrase':
        // The new phrase is sealed under the current account key, so the stale flag clears.
        this.sync = { ...this.sync, recoveryStale: false };
        return {
          type: 'RecoveryPhraseCreated',
          payload: {
            words: Array.from({ length: 24 }, () => 'abandon'),
          },
        };

      case 'VerifyRecoveryPhrase': {
        if (command.params.words.length !== 24) {
          throw new MockEngineError(
            'recovery_rejected',
            'the phrase has 24 words',
            'word-count',
          );
        }
        if (command.params.words.some((word) => word.trim() === '')) {
          throw new MockEngineError('recovery_rejected', 'the phrase is not this device', null);
        }
        return { type: 'RecoveryVerified' };
      }

      case 'RecoverFromPhrase': {
        if (command.params.words.length !== 24) {
          throw new MockEngineError('recovery_rejected', 'the phrase has 24 words', 'word-count');
        }
        if (command.params.words.some((word) => word.trim() === '')) {
          throw new MockEngineError('recovery_rejected', 'the phrase does not open the escrow', null);
        }
        const url = command.params.url.trim();
        // The pin is captured on this first connection, exactly as `ConfigureSync` would.
        const pin = /^https:\/\//i.test(url) ? DEMO_SYNC_PIN : null;
        this.sync = {
          ...this.sync,
          mode: 'prefs',
          url,
          pin,
          tokenConfigured: command.params.token.trim() !== '',
          lastError: null,
        };
        return {
          type: 'Recovered',
          payload: {
            layers: [
              { key: 'settings', enabled: true, updated_at_ms: Date.now() },
              { key: 'tags', enabled: true, updated_at_ms: Date.now() },
              { key: 'catalogue', enabled: true, updated_at_ms: Date.now() },
              { key: 'backgrounds', enabled: false, updated_at_ms: 0 },
              { key: 'theme', enabled: true, updated_at_ms: Date.now() },
              { key: 'messages', enabled: false, updated_at_ms: 0 },
            ],
            devices: this.devices,
          },
        };
      }

      case 'ListDevices':
        return { type: 'DevicesList', payload: { devices: this.devices } };

      case 'RevokeDevice': {
        const target = this.devices.find((device) => device.device_id === command.params.device_id);
        if (!target) throw notFound(`no device ${command.params.device_id}`);
        if (target.current) throw invalidArgument('this device cannot revoke itself');
        this.devices = this.devices.filter((device) => device !== target);
        this.epoch += 1;
        // The dev walk keeps two previous-epoch blobs and a now-stale phrase, so both warnings
        // and the recovery remedy are reachable without a real server.
        this.sync = { ...this.sync, recoveryStale: true };
        return {
          type: 'DeviceRevoked',
          payload: {
            epoch: this.epoch,
            re_sealed: true,
            blobs_deleted: 3,
            blobs_remaining: 2,
            recovery_stale: true,
            error: null,
          },
        };
      }

      case 'ConfigureSync': {
        const url = command.params.url.trim();
        const mode = command.params.mode;
        if (mode === 'prefs' && url === '') {
          throw invalidArgument('a sync mode of prefs needs a server URL');
        }
        if (mode === 'prefs' && !/^https:\/\//i.test(url)) {
          throw invalidArgument('the sync URL must start with https://');
        }
        // A stable demonstration pin, derived from nothing real: the dev walk only needs a
        // fingerprint to display and compare.
        const pin = mode === 'prefs' ? DEMO_SYNC_PIN : null;
        if (
          mode === 'prefs' &&
          this.sync.pin !== null &&
          pin !== this.sync.pin &&
          !command.params.accept_new_pin
        ) {
          throw new MockEngineError(
            'pin_mismatch',
            `the sync endpoint certificate changed: pinned ${this.sync.pin}, presented ${pin}`,
            'pin_mismatch'
          );
        }
        this.sync = {
          ...this.sync,
          mode,
          url: url === '' ? null : url,
          pin,
          tokenConfigured: this.sync.tokenConfigured || command.params.token.trim() !== '',
          lastError: null,
        };
        return {
          type: 'SyncConfigured',
          payload: { pin, status: this.syncReport() },
        };
      }

      case 'SyncStatus':
        return { type: 'SyncStatus', payload: { status: this.syncReport() } };

      case 'SyncNow': {
        if (this.sync.mode === 'off') {
          return {
            type: 'SyncCompleted',
            payload: {
              uploaded: 0,
              downloaded: 0,
              head_hex: '0'.repeat(64),
              epoch: this.epoch,
            },
          };
        }
        this.sync = { ...this.sync, state: 'running', lastError: null };
        this.emit({ type: 'SyncStateChanged', payload: { state: 'running', detail: null } });
        const uploaded = 2048;
        const downloaded = 1024;
        const head = 'ab'.repeat(32);
        this.sync = { ...this.sync, state: 'done', lastSyncMs: Date.now() };
        this.emit({ type: 'SyncStateChanged', payload: { state: 'done', detail: null } });
        return {
          type: 'SyncCompleted',
          payload: { uploaded, downloaded, head_hex: head, epoch: this.epoch },
        };
      }

      default: {
        const unhandled: never = command;
        throw invalidArgument(`unhandled command ${JSON.stringify(unhandled)}`);
      }
    }
  }
}
