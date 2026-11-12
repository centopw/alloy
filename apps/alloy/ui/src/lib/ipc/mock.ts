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
  RoomId,
  RoomMediaItem,
  RoomMediaKind,
  RoomMemberDto,
  RoomSearchHit,
  RoomSummary,
  RoomTagDef,
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

/** Messenger cookies the demo asks for; `datr` is optional, the other two are not. */
const MESSENGER_COOKIES: CookieFieldDto[] = [
  { name: 'c_user', label: 'c_user (the numeric id in your Facebook profile URL)', required: true },
  { name: 'xs', label: 'xs (the session cookie)', required: true },
  { name: 'datr', label: 'datr (the browser cookie)', required: false },
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

  /** `ListNetworks`: Telegram is always offered, ready only once its pair is stored. */
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
              fields: [
                { key: 'api_id', label: 'API ID', secret: false },
                { key: 'api_hash', label: 'API hash', secret: true },
              ],
            },
      },
      { network: 'messenger', ready: true, setup: null },
      { network: 'fake', ready: true, setup: null },
    ];
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

      case 'SetRoomPinned':
        this.roomById(command.params.room_id);
        this.touchRoom(command.params.room_id, { pinned: command.params.pinned });
        return { type: 'RoomFlagsUpdated' };

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

      default: {
        const unhandled: never = command;
        throw invalidArgument(`unhandled command ${JSON.stringify(unhandled)}`);
      }
    }
  }
}
