import type {
  EngineCommand,
  EngineResponse,
  EngineEvent,
  RoomSummary,
  RoomTagDef,
  TimelineItemDto,
  Account,
  SasEmoji,
  VectorDiffDto,
  RoomMediaItem,
  RoomMediaKind,
  RoomMemberDto,
  RoomMembership,
  UserProfileDto,
  NotificationLevel,
  NotificationSettingsDto,
  OwnProfileDto,
  DeviceDto,
  SessionInfoDto,
  EngineError,
} from '../generated/index.js';
import type { Transport, EventHandler, Unsubscribe } from './transport.js';
import { makeTagId } from '../stores/tags.js';
type SearchRoomMessagesParams = Extract<EngineCommand, { action: 'SearchRoomMessages' }>['params'];
type RoomSearchResults = Extract<EngineResponse, { type: 'RoomSearchResults' }>['payload'];
type RoomSearchHit = RoomSearchResults['hits'][number];

const MOCK_ACCOUNT: Account = {
  id: 'acc_alice',
  user_id: '@alice:alloy.chat',
  display_name: 'Alice',
  avatar_url: null,
  homeserver_url: 'https://matrix.alloy.chat',
};

const MOCK_DEVICE_ID = 'MOCKDEVICE';

const MOCK_DEVICES: DeviceDto[] = [
  {
    device_id: MOCK_DEVICE_ID,
    display_name: 'Alloy Desktop (macOS)',
    last_seen_ts: 1_792_314_000_000,
    last_seen_ip: '192.0.2.14',
    is_current: true,
    verified: true,
  },
  {
    device_id: 'MOCKPHONE01',
    display_name: 'Alloy Android',
    last_seen_ts: 1_792_268_100_000,
    last_seen_ip: '203.0.113.42',
    is_current: false,
    verified: false,
  },
  {
    device_id: 'MOCKLAPTOP02',
    display_name: 'Old ThinkPad (Firefox)',
    last_seen_ts: 1_788_090_120_000,
    last_seen_ip: null,
    is_current: false,
    verified: true,
  },
];

const MOCK_SESSION_INFO: SessionInfoDto = {
  device_id: MOCK_DEVICE_ID,
  ed25519_key: 'Hq3sZ0tYvW8kXb1RmN5pLc7dAeF2gJu4oIy6TrBwCxE',
  curve25519_key: 'Qm8fLr2NdV6wYc0ZkH4sTa9BpXe1UjG7oRi3KlWvCyA',
  cross_signing_ready: true,
  backup_enabled: true,
  verified: true,
};

function invalidArgument(message: string): EngineError {
  return { code: 'invalid_argument', message, reason: null };
}

const MOCK_MEMBERS: RoomMemberDto[] = [
  { user_id: '@alice:alloy.chat', display_name: 'Alice', avatar_url: null, power_level: 100, membership: 'Joined' },
  { user_id: '@bob:alloy.chat', display_name: 'Bob', avatar_url: null, power_level: 50, membership: 'Joined' },
  { user_id: '@carol:alloy.chat', display_name: null, avatar_url: null, power_level: 0, membership: 'Invited' },
];

const PROFILE_MEMBERSHIP: Record<RoomMembership, string> = {
  Joined: 'join',
  Invited: 'invite',
  Left: 'leave',
  Banned: 'ban',
};

const MOCK_MEDIA_COUNT = 14;
const MOCK_MEDIA_NEWEST_MILLIS = 1_767_225_600_000;

/** Deterministic shared items of one kind, newest first. */
function mockMediaItems(kind: RoomMediaKind): RoomMediaItem[] {
  return Array.from({ length: MOCK_MEDIA_COUNT }, (_, i) => {
    const fromAlice = i % 2 === 0;
    const base = {
      event_id: `$mock_${kind.toLowerCase()}_${i}`,
      sender: fromAlice ? '@alice:alloy.chat' : '@bob:alloy.chat',
      sender_display_name: fromAlice ? 'Alice' : 'Bob',
      timestamp_millis: MOCK_MEDIA_NEWEST_MILLIS - i * 3_600_000,
      uri: null,
      thumbnail_uri: null,
      mime_type: null,
      size_bytes: null,
      width: null,
      height: null,
    };
    if (kind === 'Links') {
      return {
        ...base,
        kind: 'link',
        body: `Shared link ${i}`,
        urls: [`https://example.org/article-${i}`],
      };
    }
    if (kind === 'Files') {
      const audio = i % 2 === 1;
      return {
        ...base,
        kind: audio ? 'audio' : 'file',
        body: audio ? `voice-${i}.ogg` : `report-${i}.pdf`,
        uri: `mxc://alloy.chat/file-${i}`,
        mime_type: audio ? 'audio/ogg' : 'application/pdf',
        size_bytes: 20_480 + i * 1_024,
      };
    }
    const video = i % 2 === 1;
    return {
      ...base,
      kind: video ? 'video' : 'image',
      body: video ? `clip-${i}.mp4` : `photo-${i}.jpg`,
      uri: `mxc://alloy.chat/media-${i}`,
      mime_type: video ? 'video/mp4' : 'image/jpeg',
      size_bytes: 300_000 + i * 5_000,
      width: 1280,
      height: 720,
    };
  });
}

const MOCK_ROOMS: RoomSummary[] = [
  {
    id: '!general:alloy.chat',
    name: 'General Discussion',
    avatar_url: null,
    unread_notifications: {
      highlight_count: 0,
      notification_count: 3,
    },
    latest_event: {
      sender: '@bob:alloy.chat',
      timestamp_millis: Date.now() - 60000,
      body: 'Welcome to Alloy chat!',
    },
    is_direct: false,
    is_space: false,
    encrypted: true,
    membership: 'Joined',
    favourite: null,
    low_priority: null,
    tags: [],
    direct_user_id: null,
  },
  {
    id: '!dev:alloy.chat',
    name: 'Alloy Core Dev',
    avatar_url: null,
    unread_notifications: {
      highlight_count: 1,
      notification_count: 5,
    },
    latest_event: {
      sender: '@alice:alloy.chat',
      timestamp_millis: Date.now() - 120000,
      body: 'Sliding sync native benchmarks look great.',
    },
    is_direct: false,
    is_space: false,
    encrypted: true,
    membership: 'Joined',
    favourite: true,
    low_priority: null,
    tags: [],
    direct_user_id: null,
  },
  {
    id: '!bob:alloy.chat',
    name: 'Bob',
    avatar_url: null,
    unread_notifications: {
      highlight_count: 0,
      notification_count: 0,
    },
    latest_event: {
      sender: '@bob:alloy.chat',
      timestamp_millis: Date.now() - 3600000,
      body: 'Hey Alice, did you review the PR?',
    },
    is_direct: true,
    is_space: false,
    encrypted: true,
    membership: 'Joined',
    favourite: null,
    low_priority: null,
    tags: [],
    direct_user_id: null,
  },
];

const MOCK_SAS_EMOJIS: SasEmoji[] = [
  { emoji: '🐶', decimal: 1234, description: 'Dog' },
  { emoji: '🐱', decimal: 2345, description: 'Cat' },
  { emoji: '🦁', decimal: 3456, description: 'Lion' },
  { emoji: '🐎', decimal: 4567, description: 'Horse' },
  { emoji: '🦄', decimal: 5678, description: 'Unicorn' },
  { emoji: '🐬', decimal: 6789, description: 'Dolphin' },
  { emoji: '🐘', decimal: 7890, description: 'Elephant' },
];

interface MockPoll {
  question: string;
  answers: { id: string; text: string }[];
  disclosed: boolean;
  maxSelections: number;
  hasEnded: boolean;
  endTime: number | null;
  myVotes: string[];
  tallies: Map<string, number>;
}

/**
 * In-memory Mock Transport for UI development and unit testing.
 * Provides realistic fake Matrix sliding-sync diffs, local echo state transitions,
 * and an interactive device verification (SAS) flow.
 */
export class MockTransport implements Transport {
  private handlers = new Set<EventHandler>();
  private rooms: RoomSummary[] = [...MOCK_ROOMS];
  private timelineByRoom = new Map<string, TimelineItemDto[]>();
  private roomSubscriptions = new Set<string>();
  private timelineSubscriptions = new Map<string, string>(); // sub_id -> room_id
  private subCounter = 0;
  private polls = new Map<string, MockPoll>();
  private topics = new Map<string, string>();
  private notes = new Map<string, string>();
  private dmRooms = new Map<string, string>([['@bob:alloy.chat', '!bob:alloy.chat']]);
  private draftsByRoom = new Map<string, { text: string; reply_to?: string | null; edit_of?: string | null }>();
  private pinnedsByRoom = new Map<string, string[]>(); // room_id -> list of event_ids (oldest first)
  private roomFavourites = new Map<string, boolean>(); // room_id -> is_favourite
  private roomLowPriorities = new Map<string, boolean>(); // room_id -> is_low_priority
  private tagCatalog: RoomTagDef[] = [];
  /** Rooms whose timeline was closed: reopening one drops this mark. */
  private closedTimelines = new Set<string>();
  private notificationSettings: NotificationSettingsDto = {
    one_to_one: 'All',
    group: 'Mentions',
    room_mention: true,
    user_mention: true,
    invite: true,
  };
  private roomNotificationLevels = new Map<string, NotificationLevel>();
  private ownProfile: OwnProfileDto = {
    user_id: MOCK_ACCOUNT.user_id,
    display_name: MOCK_ACCOUNT.display_name,
    avatar_url: MOCK_ACCOUNT.avatar_url,
    device_id: MOCK_DEVICE_ID,
    homeserver: MOCK_ACCOUNT.homeserver_url,
  };
  private mediaCacheBytes = 42_991_616;
  private oidcTimer: ReturnType<typeof setTimeout> | null = null;

  /** Drops a pending mock OIDC sign-in so a cancel or a second attempt cannot sign in later. */
  private cancelMockOidc(): void {
    if (this.oidcTimer !== null) {
      clearTimeout(this.oidcTimer);
      this.oidcTimer = null;
    }
  }

  /** Default level for a room, by its direct flag: one-to-one or group. */
  private defaultRoomLevel(roomId: string): NotificationLevel {
    const room = this.rooms.find((r) => r.id === roomId);
    return room?.is_direct ? this.notificationSettings.one_to_one : this.notificationSettings.group;
  }

  /** Applies a patch to a known room and pushes a Set diff to every room-list subscriber. */
  private patchRoom(roomId: string, patch: Partial<RoomSummary>): void {
    const index = this.rooms.findIndex((r) => r.id === roomId);
    if (index === -1) return;
    const value = { ...this.rooms[index], ...patch };
    this.rooms[index] = value;
    for (const subId of this.roomSubscriptions) {
      this.emit({
        type: 'RoomListDiff',
        payload: { subscription_id: subId, diff: { type: 'Set', content: { index, value } } },
      });
    }
  }

  /** Builds the poll content as the engine maps it: counts stay hidden until the poll ends when undisclosed. */
  private pollItemContent(eventId: string): TimelineItemDto['content'] {
    const poll = this.polls.get(eventId);
    if (!poll) throw new Error(`unknown poll ${eventId}`);
    const reveal = poll.disclosed || poll.hasEnded;
    const answers = poll.answers.map((a) => ({
      id: a.id,
      text: a.text,
      votes: reveal ? (poll.tallies.get(a.id) ?? 0) : 0,
    }));
    return {
      kind: 'Poll',
      data: {
        question: poll.question,
        answers,
        disclosed: poll.disclosed,
        max_selections: poll.maxSelections,
        total_votes: answers.reduce((sum, a) => sum + a.votes, 0),
        has_ended: poll.hasEnded,
        end_time: poll.endTime,
        my_votes: [...poll.myVotes],
      },
    };
  }

  private emitPollUpdate(roomId: string, eventId: string) {
    const items = this.timelineByRoom.get(roomId) ?? [];
    const idx = items.findIndex((i) => i.event_id === eventId);
    if (idx === -1) return;
    items[idx] = { ...items[idx], content: this.pollItemContent(eventId) };
    for (const [subId, rId] of this.timelineSubscriptions.entries()) {
      if (rId === roomId) {
        this.emit({
          type: 'TimelineDiff',
          payload: {
            subscription_id: subId,
            room_id: roomId,
            diff: { type: 'Set', content: { index: idx, value: items[idx] } },
          },
        });
      }
    }
  }

  constructor() {
    this.seedTimelines();
  }

  /** A short, believable conversation so the demo (and the README screenshots) look alive. */
  private demoConversation(): TimelineItemDto[] {
    const now = Date.now();
    const text = (n: number, sender: string, body: string, minutesAgo: number, extra: Partial<TimelineItemDto> = {}): TimelineItemDto => ({
      internal_id: `demo_${n}`,
      event_id: `$demo_${n}:alloy.chat`,
      sender,
      timestamp_millis: now - minutesAgo * 60_000,
      content: { kind: 'TextMessage', data: { body, formatted_body: null, html_sanitized: true } },
      send_state: { status: 'Sent', data: { event_id: `$demo_${n}:alloy.chat` } },
      is_own: sender === MOCK_ACCOUNT.user_id,
      reply_to: null,
      thread_summary: null,
      read_receipts: null,
      mentions_me: false,
      ...extra,
    });
    const items = [
      text(1, '@bob:alloy.chat', 'Morning! The release build finished: macOS, Windows, Linux and the Android APK.', 42),
      text(2, '@charlie:alloy.chat', 'Nice. Did the update check pick it up on your machine?', 40),
      text(3, MOCK_ACCOUNT.user_id, 'Yes, it shows the changelog and a "Restart to apply update" button after the download.', 38),
      text(4, '@bob:alloy.chat', 'I tagged the rooms we use for the launch so they sort together. Search finds old messages too.', 25),
      text(5, '@charlie:alloy.chat', 'Great, shipping the notes to the team channel now.', 12),
    ];
    items[2].content = {
      kind: 'TextMessage',
      data: {
        body: 'Yes, it shows the changelog and a "Restart to apply update" button after the download.',
        formatted_body: '<p>Yes, it shows the <strong>changelog</strong> and a "Restart to apply update" button after the download.</p>',
        html_sanitized: true,
      },
    };
    items[2].reactions = [{ key: '\u{1F44D}', count: 2, senders: ['@bob:alloy.chat', '@charlie:alloy.chat'], by_me: false }];
    items[4].reply_to = items[3];
    return items;
  }

  private seedTimelines() {
    const generalItems: TimelineItemDto[] = [
      {
        internal_id: 'item_1',
        event_id: '$msg_1:alloy.chat',
        sender: '@system:alloy.chat',
        timestamp_millis: Date.now() - 86400000,
        content: {
          kind: 'DayDivider',
          data: {
            timestamp_millis: Date.now() - 86400000,
          },
        },
        send_state: { status: 'Sent', data: { event_id: '$msg_1:alloy.chat' } },
        is_own: false,
        reply_to: null,
        thread_summary: null,
        read_receipts: null,
        mentions_me: false,
      },
      {
        internal_id: 'item_2',
        event_id: '$msg_2:alloy.chat',
        sender: '@bob:alloy.chat',
        timestamp_millis: Date.now() - 3600000,
        content: {
          kind: 'TextMessage',
          data: {
            body: 'Welcome to Alloy Matrix Client prototype!',
            formatted_body: '<p>Welcome to <strong>Alloy</strong> Matrix Client prototype!</p>',
            html_sanitized: true,
          },
        },
        send_state: { status: 'Sent', data: { event_id: '$msg_2:alloy.chat' } },
        is_own: false,
        reply_to: null,
        thread_summary: null,
        read_receipts: ['@alice:alloy.chat'],
        mentions_me: false,
      },
      {
        internal_id: 'item_3',
        event_id: '$msg_3:alloy.chat',
        sender: '@alice:alloy.chat',
        timestamp_millis: Date.now() - 1800000,
        content: {
          kind: 'TextMessage',
          data: {
            body: 'Thanks Bob! The virtualized timeline is rendering smoothly at 60 fps.',
            formatted_body: null,
            html_sanitized: true,
          },
        },
        send_state: { status: 'Sent', data: { event_id: '$msg_3:alloy.chat' } },
        is_own: true,
        reply_to: null,
        thread_summary: null,
        read_receipts: ['@bob:alloy.chat'],
        mentions_me: false,
      },
    ];

    this.timelineByRoom.set('!general:alloy.chat', generalItems);
    this.timelineByRoom.set('!dev:alloy.chat', this.demoConversation());
    this.timelineByRoom.set('!bob:alloy.chat', []);
  }

  subscribe(handler: EventHandler): Unsubscribe {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  emit(event: EngineEvent) {
    for (const handler of this.handlers) {
      try {
        handler(event);
      } catch (err) {
        console.error('[MockTransport] Error in subscriber:', err);
      }
    }
  }

  /** Encrypted rooms report Unavailable, as the homeserver cannot search them; the UI then searches locally. */
  private searchRoomMessages(params: SearchRoomMessagesParams): RoomSearchResults {
    const room = this.rooms.find((r) => r.id === params.room_id);
    if (!room || room.encrypted) return { hits: [], next_batch: null, scope: 'Unavailable' };
    const needle = params.query.toLowerCase();
    const hits: RoomSearchHit[] = [];
    for (const item of this.timelineByRoom.get(params.room_id) ?? []) {
      if (item.event_id === null || item.content.kind !== 'TextMessage') continue;
      const body = item.content.data.body;
      if (!body.toLowerCase().includes(needle)) continue;
      if (params.sender !== null && item.sender !== params.sender) continue;
      hits.push({
        event_id: item.event_id,
        sender: item.sender,
        sender_display_name: item.sender_display_name ?? null,
        timestamp_millis: item.timestamp_millis,
        body,
      });
    }
    // Newest first, as the homeserver returns them.
    hits.reverse();
    return { hits: hits.slice(0, params.limit), next_batch: null, scope: 'Server' };
  }

  async invoke(command: EngineCommand): Promise<EngineResponse> {
    if (command.action === 'SearchRoomMessages') {
      return { type: 'RoomSearchResults', payload: this.searchRoomMessages(command.params) };
    }
    switch (command.action) {
      case 'DiscoverHomeserver':
        return {
          type: 'HomeserverInfo',
          payload: {
            url: command.params.input.startsWith('http')
              ? command.params.input
              : `https://${command.params.input}`,
            name: 'Alloy Matrix Server',
            supports_oidc: true,
          },
        };

      case 'LoginPassword':
        return {
          type: 'LoggedIn',
          payload: {
            account: {
              ...MOCK_ACCOUNT,
              user_id: command.params.username.startsWith('@')
                ? command.params.username
                : `@${command.params.username}:alloy.chat`,
              homeserver_url: command.params.homeserver_url,
            },
          },
        };

      case 'StartOidcLogin':
        // No browser exists to authorize in: answer with a URL the opener ignores and sign in
        // shortly after, so the waiting screen and the signed-in shell stay walkable.
        this.cancelMockOidc();
        this.oidcTimer = setTimeout(() => {
          this.oidcTimer = null;
          this.emit({ type: 'AuthState', payload: { account: MOCK_ACCOUNT } });
        }, 700);
        return {
          type: 'OidcUrl',
          payload: {
            authorization_url: 'alloy-mock://oidc',
            redirect_uri: 'http://127.0.0.1:17897/callback',
          },
        };

      case 'CompleteOidcLogin':
        return {
          type: 'LoggedIn',
          payload: { account: MOCK_ACCOUNT },
        };

      case 'CancelOidcLogin':
        this.cancelMockOidc();
        return { type: 'Unit' };

      case 'RestoreSession':
        return {
          type: 'SessionRestored',
          payload: { account: MOCK_ACCOUNT },
        };

      case 'Logout':
        return { type: 'LoggedOut' };

      case 'ListAccounts':
        return {
          type: 'AccountsList',
          payload: { accounts: [MOCK_ACCOUNT] },
        };

      case 'SubscribeRoomList': {
        const subId = `sub_rooms_${++this.subCounter}`;
        this.roomSubscriptions.add(subId);

        // Stream initial room list diff via Reset
        setTimeout(() => {
          this.emit({
            type: 'RoomListDiff',
            payload: {
              subscription_id: subId,
              diff: {
                type: 'Reset',
                content: { values: [...this.rooms] },
              },
            },
          });
        }, 10);

        return {
          type: 'RoomListSubscribed',
          payload: { subscription_id: subId },
        };
      }

      case 'Unsubscribe': {
        this.roomSubscriptions.delete(command.params.subscription_id);
        this.timelineSubscriptions.delete(command.params.subscription_id);
        return {
          type: 'Unsubscribed',
          payload: { subscription_id: command.params.subscription_id },
        };
      }

      case 'JoinRoom': {
        const newRoomId = command.params.room_id_or_alias.startsWith('!')
          ? command.params.room_id_or_alias
          : `!${command.params.room_id_or_alias}:alloy.chat`;

        const newRoom: RoomSummary = {
          id: newRoomId,
          name: command.params.room_id_or_alias,
          avatar_url: null,
          unread_notifications: { highlight_count: 0, notification_count: 0 },
          latest_event: null,
          is_direct: false,
          is_space: false,
          encrypted: true,
          membership: 'Joined',
          favourite: null,
          low_priority: null,
          tags: [],
          direct_user_id: null,
        };

        this.rooms.push(newRoom);
        if (!this.timelineByRoom.has(newRoomId)) {
          this.timelineByRoom.set(newRoomId, []);
        }

        // Notify room subscribers
        for (const subId of this.roomSubscriptions) {
          this.emit({
            type: 'RoomListDiff',
            payload: {
              subscription_id: subId,
              diff: { type: 'PushBack', content: { value: newRoom } },
            },
          });
        }

        return {
          type: 'RoomJoined',
          payload: { room_id: newRoomId },
        };
      }

      case 'LeaveRoom': {
        this.rooms = this.rooms.filter((r) => r.id !== command.params.room_id);
        return {
          type: 'RoomLeft',
          payload: { room_id: command.params.room_id },
        };
      }

      case 'CreateRoom': {
        const roomId = `!created_${Date.now()}:alloy.chat`;
        const newRoom: RoomSummary = {
          id: roomId,
          name: command.params.name,
          avatar_url: null,
          unread_notifications: { highlight_count: 0, notification_count: 0 },
          latest_event: null,
          is_direct: false,
          is_space: false,
          encrypted: command.params.encrypted,
          membership: 'Joined',
          favourite: null,
          low_priority: null,
          tags: [],
          direct_user_id: null,
        };

        this.rooms.push(newRoom);
        this.timelineByRoom.set(roomId, []);

        for (const subId of this.roomSubscriptions) {
          this.emit({
            type: 'RoomListDiff',
            payload: {
              subscription_id: subId,
              diff: { type: 'PushBack', content: { value: newRoom } },
            },
          });
        }

        return {
          type: 'RoomCreated',
          payload: { room_id: roomId },
        };
      }

      case 'SubscribeTimeline': {
        const subId = `sub_tl_${++this.subCounter}`;
        const roomId = command.params.room_id;
        this.timelineSubscriptions.set(subId, roomId);
        // Subscribing is what reopens a room's timeline after it was closed.
        this.closedTimelines.delete(roomId);

        const items = this.timelineByRoom.get(roomId) ?? [];

        // Stream initial items
        setTimeout(() => {
          this.emit({
            type: 'TimelineDiff',
            payload: {
              subscription_id: subId,
              room_id: roomId,
              diff: {
                type: 'Reset',
                content: { values: [...items] },
              },
            },
          });
        }, 10);

        return {
          type: 'TimelineSubscribed',
          payload: { subscription_id: subId },
        };
      }

      case 'SubscribeThread': {
        const roomId = command.params.room_id;
        const root = command.params.thread_root_event_id;
        const subId = `sub_th_${++this.subCounter}`;
        // Thread subscriptions use a composite key, so room-level diffs skip them.
        this.timelineSubscriptions.set(subId, `${roomId}|thread|${root}`);

        const items = (this.timelineByRoom.get(roomId) ?? []).filter(
          (i) => i.event_id === root || i.reply_to?.event_id === root,
        );

        setTimeout(() => {
          this.emit({
            type: 'TimelineDiff',
            payload: {
              subscription_id: subId,
              room_id: roomId,
              diff: {
                type: 'Reset',
                content: { values: [...items] },
              },
            },
          });
        }, 10);

        return {
          type: 'TimelineSubscribed',
          payload: { subscription_id: subId },
        };
      }

      case 'PaginateBackwards':
        // The demo rooms hold their whole history already.
        return { type: 'Paginated', payload: { room_id: command.params.room_id, reached_start: true } };

      case 'SendText': {
        const roomId = command.params.room_id;
        const txnId = command.params.txn_id ?? `txn_${Date.now()}`;
        const internalId = `local_${txnId}`;
        const threadRoot = command.params.in_thread_root ?? null;
        const threadKey = threadRoot ? `${roomId}|thread|${threadRoot}` : null;
        const rootItem = threadRoot
          ? ((this.timelineByRoom.get(roomId) ?? []).find((i) => i.event_id === threadRoot) ?? null)
          : null;

        const localEcho: TimelineItemDto = {
          internal_id: internalId,
          event_id: null,
          sender: MOCK_ACCOUNT.user_id,
          timestamp_millis: Date.now(),
          content: {
            kind: 'TextMessage',
            data: {
              body: command.params.body,
              formatted_body: null,
              html_sanitized: true,
            },
          },
          send_state: { status: 'Sending', data: { transaction_id: txnId } },
          is_own: true,
          reply_to: rootItem,
          thread_summary: null,
          read_receipts: null,
          mentions_me: false,
        };

        const currentItems = this.timelineByRoom.get(roomId) ?? [];
        currentItems.push(localEcho);
        this.timelineByRoom.set(roomId, currentItems);

        // Emit local echo PushBack
        for (const [subId, rId] of this.timelineSubscriptions.entries()) {
          if (threadKey ? rId === threadKey : rId === roomId) {
            this.emit({
              type: 'TimelineDiff',
              payload: {
                subscription_id: subId,
                room_id: roomId,
                diff: { type: 'PushBack', content: { value: localEcho } },
              },
            });
          }
        }

        const eventId = `$sent_${Date.now()}:alloy.chat`;

        // Simulate local echo resolution (Sending -> Sent) after 40ms
        setTimeout(() => {
          const sentItem: TimelineItemDto = {
            ...localEcho,
            event_id: eventId,
            send_state: { status: 'Sent', data: { event_id: eventId } },
          };

          const items = this.timelineByRoom.get(roomId) ?? [];
          const idx = items.findIndex((i) => i.internal_id === internalId);
          if (idx !== -1) {
            items[idx] = sentItem;
            for (const [subId, rId] of this.timelineSubscriptions.entries()) {
              if (rId === roomId) {
                this.emit({
                  type: 'TimelineDiff',
                  payload: {
                    subscription_id: subId,
                    room_id: roomId,
                    diff: { type: 'Set', content: { index: idx, value: sentItem } },
                  },
                });
              }
            }
          }
        }, 40);

        return {
          type: 'MessageSent',
          payload: {
            room_id: roomId,
            txn_id: txnId,
            event_id: eventId,
          },
        };
      }

      case 'EditMessage': {
        const roomId = command.params.room_id;
        const items = this.timelineByRoom.get(roomId) ?? [];
        const idx = items.findIndex((i) => i.event_id === command.params.event_id);
        if (idx !== -1) {
          const item = items[idx];
          const updated: TimelineItemDto = {
            ...item,
            content: {
              kind: 'TextMessage',
              data: {
                body: command.params.body,
                formatted_body: null,
                html_sanitized: true,
              },
            },
          };
          items[idx] = updated;

          for (const [subId, rId] of this.timelineSubscriptions.entries()) {
            if (rId === roomId) {
              this.emit({
                type: 'TimelineDiff',
                payload: {
                  subscription_id: subId,
                  room_id: roomId,
                  diff: { type: 'Set', content: { index: idx, value: updated } },
                },
              });
            }
          }
        }

        return {
          type: 'MessageEdited',
          payload: {
            room_id: roomId,
            event_id: command.params.event_id,
          },
        };
      }

      case 'RedactEvent': {
        const roomId = command.params.room_id;
        const items = this.timelineByRoom.get(roomId) ?? [];
        const idx = items.findIndex((i) => i.event_id === command.params.event_id);
        if (idx !== -1) {
          const item = items[idx];
          const redacted: TimelineItemDto = {
            ...item,
            content: { kind: 'Redacted' },
          };
          items[idx] = redacted;

          for (const [subId, rId] of this.timelineSubscriptions.entries()) {
            if (rId === roomId) {
              this.emit({
                type: 'TimelineDiff',
                payload: {
                  subscription_id: subId,
                  room_id: roomId,
                  diff: { type: 'Set', content: { index: idx, value: redacted } },
                },
              });
            }
          }
        }

        return {
          type: 'EventRedacted',
          payload: {
            room_id: roomId,
            event_id: command.params.event_id,
          },
        };
      }

      case 'ToggleReaction': {
        return {
          type: 'ReactionToggled',
          payload: {
            room_id: command.params.room_id,
            event_id: command.params.event_id,
            key: command.params.key,
            added: true,
          },
        };
      }

      case 'SendPoll': {
        const roomId = command.params.room_id;
        const eventId = `$poll_${Date.now()}:alloy.chat`;
        this.polls.set(eventId, {
          question: command.params.question.trim(),
          answers: command.params.answers.map((text, i) => ({ id: String(i), text: text.trim() })),
          disclosed: command.params.disclosed,
          maxSelections: command.params.max_selections,
          hasEnded: false,
          endTime: null,
          myVotes: [],
          tallies: new Map(),
        });
        const item: TimelineItemDto = {
          internal_id: `local_${eventId}`,
          event_id: eventId,
          sender: MOCK_ACCOUNT.user_id,
          timestamp_millis: Date.now(),
          content: this.pollItemContent(eventId),
          send_state: { status: 'Sent', data: { event_id: eventId } },
          is_own: true,
          reply_to: null,
          thread_summary: null,
          read_receipts: null,
          mentions_me: false,
        };
        const items = this.timelineByRoom.get(roomId) ?? [];
        items.push(item);
        this.timelineByRoom.set(roomId, items);
        for (const [subId, rId] of this.timelineSubscriptions.entries()) {
          if (rId === roomId) {
            this.emit({
              type: 'TimelineDiff',
              payload: {
                subscription_id: subId,
                room_id: roomId,
                diff: { type: 'PushBack', content: { value: item } },
              },
            });
          }
        }
        return {
          type: 'MessageSent',
          payload: { room_id: roomId, txn_id: `poll_${Date.now()}`, event_id: null },
        };
      }

      case 'RespondToPoll': {
        const poll = this.polls.get(command.params.poll_event_id);
        if (poll && !poll.hasEnded) {
          for (const id of poll.myVotes) {
            poll.tallies.set(id, Math.max(0, (poll.tallies.get(id) ?? 0) - 1));
          }
          const picked = [...new Set(command.params.answer_ids)]
            .filter((id) => poll.answers.some((a) => a.id === id))
            .slice(0, poll.maxSelections);
          for (const id of picked) {
            poll.tallies.set(id, (poll.tallies.get(id) ?? 0) + 1);
          }
          poll.myVotes = picked;
          this.emitPollUpdate(command.params.room_id, command.params.poll_event_id);
        }
        return { type: 'Unit' };
      }

      case 'EndPoll': {
        const poll = this.polls.get(command.params.poll_event_id);
        if (poll && !poll.hasEnded) {
          poll.hasEnded = true;
          poll.endTime = Date.now();
          this.emitPollUpdate(command.params.room_id, command.params.poll_event_id);
        }
        return { type: 'Unit' };
      }

      case 'RetrySend':
        return {
          type: 'SendRetried',
          payload: {
            room_id: command.params.room_id,
            txn_id: command.params.txn_id,
          },
        };

      case 'CancelSend':
        return {
          type: 'SendCancelled',
          payload: {
            room_id: command.params.room_id,
            txn_id: command.params.txn_id,
          },
        };

      case 'SendAttachment': {
        const eventId = `$att_${Date.now()}:alloy.chat`;
        return {
          type: 'AttachmentSent',
          payload: {
            room_id: command.params.room_id,
            txn_id: command.params.txn_id ?? `txn_${Date.now()}`,
            event_id: eventId,
          },
        };
      }

      case 'SendVoice': {
        const eventId = `$voice_${Date.now()}:alloy.chat`;
        return {
          type: 'AttachmentSent',
          payload: {
            room_id: command.params.room_id,
            txn_id: command.params.txn_id ?? `txn_${Date.now()}`,
            event_id: eventId,
          },
        };
      }

      case 'SendLocation':
        return {
          type: 'LocationSent',
          payload: { room_id: command.params.room_id },
        };

      case 'StartLiveLocation':
        return {
          type: 'LiveLocationStarted',
          payload: { room_id: command.params.room_id },
        };

      case 'SendLocationBeacon':
        return {
          type: 'LocationBeaconSent',
          payload: { room_id: command.params.room_id },
        };

      case 'StopLiveLocation':
        return {
          type: 'LiveLocationStopped',
          payload: { room_id: command.params.room_id },
        };

      case 'MarkRead':
        return {
          type: 'ReadMarked',
          payload: { room_id: command.params.room_id },
        };

      case 'MarkRoomRead':
        this.patchRoom(command.params.room_id, {
          unread_notifications: { highlight_count: 0, notification_count: 0 },
        });
        return { type: 'Unit' };

      case 'SetRoomName':
        this.patchRoom(command.params.room_id, { name: command.params.name });
        return { type: 'Unit' };

      case 'SetRoomTopic':
        this.topics.set(command.params.room_id, command.params.topic);
        return { type: 'Unit' };

      case 'SetRoomAvatar':
        this.patchRoom(command.params.room_id, {
          avatar_url: command.params.file_path ? 'mxc://alloy.chat/mock-avatar' : null,
        });
        return { type: 'Unit' };

      case 'GetTagCatalog':
        return { type: 'TagCatalog', payload: { tags: this.tagCatalog.map((t) => ({ ...t })) } };

      case 'SaveTagCatalog':
        this.tagCatalog = command.params.tags.map((t) => ({ ...t }));
        return { type: 'Unit' };

      case 'CloseTimeline': {
        const { room_id } = command.params;
        const closed = !this.closedTimelines.has(room_id);
        this.closedTimelines.add(room_id);
        return { type: 'TimelineClosed', payload: { room_id, closed } };
      }

      case 'CreateRoomTag': {
        const tag: RoomTagDef = {
          id: makeTagId(command.params.name, new Set(this.tagCatalog.map((t) => t.id))),
          name: command.params.name.trim(),
          colour: command.params.colour,
        };
        this.tagCatalog = [...this.tagCatalog, tag];
        return { type: 'RoomTagCreated', payload: { tag: { ...tag } } };
      }

      case 'RenameRoomTag': {
        const index = this.tagCatalog.findIndex((t) => t.id === command.params.id);
        if (index === -1) throw invalidArgument('no such tag');
        const current = this.tagCatalog[index];
        const next: RoomTagDef = {
          ...current,
          name: command.params.name?.trim() || current.name,
          colour: command.params.colour ?? current.colour,
        };
        this.tagCatalog = this.tagCatalog.map((t, i) => (i === index ? next : t));
        return { type: 'Unit' };
      }

      case 'ReorderRoomTags': {
        const ids = command.params.ids;
        const known = new Set(this.tagCatalog.map((t) => t.id));
        const isPermutation = ids.length === known.size && ids.every((id) => known.has(id)) && new Set(ids).size === ids.length;
        if (!isPermutation) throw invalidArgument('ids must list the catalogue exactly once');
        this.tagCatalog = ids.map((id) => this.tagCatalog.find((t) => t.id === id)!);
        return { type: 'Unit' };
      }

      case 'SetRoomTag': {
        const room = this.rooms.find((r) => r.id === command.params.room_id);
        if (room) {
          const tags = new Set(room.tags);
          if (command.params.enabled) tags.add(command.params.tag_id);
          else tags.delete(command.params.tag_id);
          this.patchRoom(room.id, { tags: [...tags] });
        }
        return { type: 'Unit' };
      }

      case 'DeleteRoomTag': {
        const tagId = command.params.tag_id;
        this.tagCatalog = this.tagCatalog.filter((t) => t.id !== tagId);
        for (const room of this.rooms.filter((r) => r.tags.includes(tagId))) {
          this.patchRoom(room.id, { tags: room.tags.filter((t) => t !== tagId) });
        }
        return { type: 'Unit' };
      }

      case 'GetRoomNote':
        return {
          type: 'RoomNote',
          payload: {
            room_id: command.params.room_id,
            note: this.notes.get(command.params.room_id) ?? '',
          },
        };

      case 'SetRoomNote':
        if (command.params.note === '') {
          this.notes.delete(command.params.room_id);
        } else {
          this.notes.set(command.params.room_id, command.params.note);
        }
        return { type: 'Unit' };

      case 'SetTyping':
        return {
          type: 'TypingStatusSet',
          payload: { room_id: command.params.room_id },
        };

      case 'GetRoomMembers':
        return {
          type: 'RoomMembers',
          payload: {
            room_id: command.params.room_id,
            members: [...MOCK_MEMBERS],
          },
        };

      case 'GetRoomDetails': {
        const roomId = command.params.room_id;
        const room = this.rooms.find((r) => r.id === roomId);
        return {
          type: 'RoomDetailsRetrieved',
          payload: {
            details: {
              room_id: roomId,
              name: room?.name ?? 'General',
              topic: this.topics.get(roomId) ?? 'Welcome to Alloy',
              canonical_alias: '#general:alloy.chat',
              avatar_url: room?.avatar_url ?? null,
              is_direct: room?.is_direct ?? false,
              encrypted: room?.encrypted ?? false,
              joined_members: 2,
              invited_members: 1,
              can_set_name: true,
              can_set_topic: true,
              can_set_avatar: true,
              can_pin: true,
              can_redact_others: true,
              can_mention_room: true,
              pinned_event_ids: this.pinnedsByRoom.get(roomId) ?? [],
            },
          },
        };
      }

      case 'GetUserProfile': {
        const userId = command.params.user_id;
        const roomId = command.params.room_id;
        const member = roomId ? MOCK_MEMBERS.find((m) => m.user_id === userId) : undefined;
        const profile: UserProfileDto = {
          user_id: userId,
          display_name: member?.display_name ?? userId.slice(1).split(':')[0] ?? userId,
          avatar_url: member?.avatar_url ?? null,
          membership: member ? PROFILE_MEMBERSHIP[member.membership] : null,
          power_level: member ? member.power_level : null,
          dm_room_id: this.dmRooms.get(userId) ?? null,
          is_me: userId === MOCK_ACCOUNT.user_id,
        };
        return { type: 'UserProfile', payload: { profile } };
      }

      case 'SearchUsers': {
        const q = command.params.query.trim().toLowerCase();
        if (!q) throw invalidArgument('search query must not be blank');
        const people = [
          ...MOCK_MEMBERS.map((m) => ({ user_id: m.user_id, display_name: m.display_name })),
          // People outside the shared rooms, so the People group shows both states.
          { user_id: '@dana:alloy.chat', display_name: 'Dana Okafor' },
          { user_id: '@erin:alloy.chat', display_name: 'Erin Blake' },
        ];
        const limit = command.params.limit;
        const matches = people.filter(
          (p) =>
            p.display_name?.toLowerCase().includes(q) ||
            p.user_id.toLowerCase().includes(q),
        );
        const results = matches.slice(0, limit).map((p) => ({
          user_id: p.user_id,
          display_name: p.display_name,
          avatar_url: null,
          already_in_room: MOCK_MEMBERS.some((m) => m.user_id === p.user_id),
        }));
        return {
          type: 'UserSearchResults',
          payload: { results, limited: matches.length > limit },
        };
      }

      case 'StartDirectMessage': {
        const userId = command.params.user_id;
        const existing = this.dmRooms.get(userId);
        if (existing) {
          return { type: 'DirectMessageRoom', payload: { room_id: existing, created: false } };
        }

        const roomId = `!dm_${userId.slice(1).split(':')[0]}:alloy.chat`;
        const newRoom: RoomSummary = {
          id: roomId,
          name: userId,
          avatar_url: null,
          unread_notifications: { highlight_count: 0, notification_count: 0 },
          latest_event: null,
          is_direct: true,
          is_space: false,
          encrypted: false,
          membership: 'Joined',
          favourite: null,
          low_priority: null,
          tags: [],
          direct_user_id: null,
        };

        this.rooms.push(newRoom);
        this.timelineByRoom.set(roomId, []);
        this.dmRooms.set(userId, roomId);

        for (const subId of this.roomSubscriptions) {
          this.emit({
            type: 'RoomListDiff',
            payload: {
              subscription_id: subId,
              diff: { type: 'PushBack', content: { value: newRoom } },
            },
          });
        }

        return { type: 'DirectMessageRoom', payload: { room_id: roomId, created: true } };
      }

      case 'GetRoomMedia': {
        const roomId = command.params.room_id;
        const offset = Number.parseInt(command.params.from ?? '0', 10) || 0;
        const all = mockMediaItems(command.params.kind);
        const end = offset + command.params.limit;
        return {
          type: 'RoomMedia',
          payload: {
            room_id: roomId,
            items: all.slice(offset, end),
            next_batch: end < all.length ? String(end) : null,
          },
        };
      }

      case 'GetMedia':
        return {
          type: 'MediaRetrieved',
          payload: { local_path: '/tmp/alloy-mock-cache/media.png' },
        };

      case 'GetUrlPreview': {
        const url = command.params.url;
        const preview = url.includes('example')
          ? {
              url,
              title: 'Example Domain',
              description: 'Mock link preview returned by the browser mock engine.',
              site_name: 'Example',
              image: null,
              image_width: null,
              image_height: null,
            }
          : null;
        return { type: 'UrlPreview', payload: { preview } };
      }

      case 'RequestSelfVerification':
        return {
          type: 'VerificationRequested',
          payload: { user_id: MOCK_ACCOUNT.user_id, device_id: MOCK_DEVICE_ID },
        };

      case 'AcceptVerification':
        return {
          type: 'VerificationAccepted',
          payload: {
            user_id: command.params.user_id,
            device_id: command.params.device_id,
          },
        };

      case 'StartSas': {
        const userId = command.params.user_id;
        const deviceId = command.params.device_id;

        setTimeout(() => {
          this.emit({
            type: 'VerificationState',
            payload: {
              user_id: userId,
              device_id: deviceId,
              state: {
                CompareEmojis: {
                  emojis: MOCK_SAS_EMOJIS,
                  decimal_1: 1234,
                  decimal_2: 5678,
                  decimal_3: 9012,
                },
              },
            },
          });
        }, 20);

        return {
          type: 'SasStarted',
          payload: {
            user_id: userId,
            device_id: deviceId,
            emojis: MOCK_SAS_EMOJIS,
          },
        };
      }

      case 'ConfirmSas': {
        const userId = command.params.user_id;
        const deviceId = command.params.device_id;

        setTimeout(() => {
          this.emit({
            type: 'VerificationState',
            payload: {
              user_id: userId,
              device_id: deviceId,
              state: 'Verified',
            },
          });
        }, 20);

        return {
          type: 'SasConfirmed',
          payload: { user_id: userId, device_id: deviceId },
        };
      }

      case 'CancelVerification': {
        const userId = command.params.user_id;
        const deviceId = command.params.device_id;

        setTimeout(() => {
          this.emit({
            type: 'VerificationState',
            payload: {
              user_id: userId,
              device_id: deviceId,
              state: { Cancelled: { reason: 'User cancelled' } },
            },
          });
        }, 20);

        return {
          type: 'VerificationCancelled',
          payload: { user_id: userId, device_id: deviceId },
        };
      }

      case 'GetRecoveryStatus':
        return {
          type: 'RecoveryStatusRetrieved',
          payload: { state: 'Enabled' },
        };

      case 'EnableRecovery':
        return {
          type: 'RecoveryEnabled',
          payload: { recovery_key: 'EsTc 1234 abcd 5678 efgh 9012 ijkl 3456 mock' },
        };

      case 'RecoverWithKey':
        return {
          type: 'RecoveredFromKey',
          payload: { recovered: true },
        };

      case 'ResetRecovery':
        return {
          type: 'RecoveryReset',
          payload: { recovery_key: 'EsTc 6543 dcba 8765 hgfe 2109 lkji 6543 mock' },
        };

      case 'GetSpaceParents':
        return { type: 'SpaceParents', payload: { parents: [] } };

      case 'ListThemes':
        return {
          type: 'ThemesList',
          payload: {
            themes: [
              ['dark', 'Dark Default'],
              ['light', 'Light Default'],
              ['amoled', 'AMOLED Pure Black'],
            ],
          },
        };

      case 'GetTheme':
        return {
          type: 'ThemeCss',
          payload: {
            css: `:root { --alloy-theme-name: "${command.params.id}"; }`,
          },
        };

      case 'SetActiveTheme':
        return { type: 'ThemeSetActive' };

      case 'SearchPublicRooms': {
        const q = (command.params.query ?? '').trim().toLowerCase();
        const catalogue = [
          { name: 'Matrix HQ', topic: 'Chat about Matrix', members: 1200 },
          { name: 'Alloy Users', topic: 'Questions and feedback', members: 42 },
        ];
        const rooms = catalogue
          .filter((r) => r.name.toLowerCase().includes(q))
          .map((r) => ({
            room_id: `!${r.name.replace(/\W+/g, '').toLowerCase()}:alloy.chat`,
            alias: `#${r.name.replace(/\W+/g, '-').toLowerCase()}:alloy.chat`,
            name: r.name,
            topic: r.topic,
            avatar_url: null,
            num_joined_members: r.members,
            world_readable: false,
            guest_can_join: false,
            joined: false,
          }));
        return {
          type: 'PublicRooms',
          payload: { rooms, next_batch: null, total_estimate: rooms.length },
        };
      }

      case 'GetSettings':
        return {
          type: 'SettingsRetrieved',
          payload: { settings_json: JSON.stringify({ notifications: true }) },
        };

      case 'SetSetting':
        return {
          type: 'SettingSet',
          payload: { key: command.params.key },
        };

      case 'GetAccountPrefs':
        return { type: 'AccountPrefs', payload: { json: null } };

      case 'SetAccountPrefs':
        return { type: 'Unit' };

      case 'GetNotificationSettings':
        return {
          type: 'NotificationSettings',
          payload: { settings: { ...this.notificationSettings } },
        };

      case 'SetDefaultNotificationLevel':
        if (command.params.one_to_one) {
          this.notificationSettings.one_to_one = command.params.level;
        } else {
          this.notificationSettings.group = command.params.level;
        }
        return { type: 'Unit' };

      case 'SetMentionRules': {
        const { room_mention, user_mention, invite } = command.params;
        if (room_mention !== null) this.notificationSettings.room_mention = room_mention;
        if (user_mention !== null) this.notificationSettings.user_mention = user_mention;
        if (invite !== null) this.notificationSettings.invite = invite;
        return { type: 'Unit' };
      }

      case 'GetRoomNotificationLevel': {
        const roomId = command.params.room_id;
        const explicit = this.roomNotificationLevels.get(roomId);
        return {
          type: 'RoomNotificationLevel',
          payload: {
            room_id: roomId,
            level: explicit ?? this.defaultRoomLevel(roomId),
            is_default: explicit === undefined,
          },
        };
      }

      case 'SetRoomNotificationLevel': {
        const roomId = command.params.room_id;
        const level = command.params.level;
        if (level === null) {
          this.roomNotificationLevels.delete(roomId);
        } else {
          this.roomNotificationLevels.set(roomId, level);
        }
        return { type: 'Unit' };
      }

      case 'GetOwnProfile':
        return { type: 'OwnProfile', payload: { profile: { ...this.ownProfile } } };

      case 'SetDisplayName': {
        const name = command.params.name.trim();
        if (name === '') {
          throw invalidArgument('display name must not be empty');
        }
        this.ownProfile = { ...this.ownProfile, display_name: name };
        return { type: 'Unit' };
      }

      case 'SetOwnAvatar':
        this.ownProfile = {
          ...this.ownProfile,
          avatar_url: command.params.file_path ? 'mxc://alloy.chat/mock-own-avatar' : null,
        };
        return { type: 'Unit' };

      case 'ListDevices':
        return { type: 'Devices', payload: { devices: MOCK_DEVICES.map((d) => ({ ...d })) } };

      case 'GetSessionInfo':
        return { type: 'SessionInfo', payload: { info: { ...MOCK_SESSION_INFO } } };

      case 'ClearMediaCache': {
        const freed = this.mediaCacheBytes;
        this.mediaCacheBytes = 0;
        return { type: 'MediaCacheCleared', payload: { freed_bytes: freed } };
      }

      case 'SetEventPinned': {
        const roomId = command.params.room_id;
        const eventId = command.params.event_id;
        const pinned = command.params.pinned;
        const pins = this.pinnedsByRoom.get(roomId) ?? [];
        if (pinned && !pins.includes(eventId)) {
          pins.push(eventId);
          this.pinnedsByRoom.set(roomId, pins);
        } else if (!pinned) {
          const idx = pins.indexOf(eventId);
          if (idx !== -1) pins.splice(idx, 1);
          this.pinnedsByRoom.set(roomId, pins);
        }
        return { type: 'EventPinned', payload: { room_id: roomId, event_id: eventId, pinned } };
      }

      case 'GetPinnedEvents': {
        const roomId = command.params.room_id;
        const pins = this.pinnedsByRoom.get(roomId) ?? [];
        const items: TimelineItemDto[] = pins.map((eid) => {
          const found = this.timelineByRoom.get(roomId)?.find((i) => i.event_id === eid);
          return found ?? {
            internal_id: `pinned_${eid}`,
            event_id: eid,
            sender: '@alice:alloy.chat',
            timestamp_millis: Date.now() - 86400000,
            content: {
              kind: 'TextMessage',
              data: {
                body: '[pinned message]',
                formatted_body: null,
                html_sanitized: false,
              },
            },
            send_state: { status: 'Sent', data: { event_id: eid } },
            is_own: false,
            reply_to: null,
            thread_summary: null,
            read_receipts: null,
            mentions_me: false,
          };
        });
        return { type: 'PinnedEvents', payload: { room_id: roomId, items } };
      }

      case 'ForwardEvents': {
        const forwarded = command.params.targets;
        const failed: { room_id: string; error: string }[] = [];
        return { type: 'EventsForwarded', payload: { forwarded, failed } };
      }

      case 'SetRoomFavourite': {
        const roomId = command.params.room_id;
        const favourite = command.params.favourite;
        this.roomFavourites.set(roomId, favourite);
        this.patchRoom(roomId, { favourite });
        return { type: 'RoomFavouriteSet', payload: { room_id: roomId, favourite } };
      }

      case 'SetRoomLowPriority': {
        const roomId = command.params.room_id;
        const low_priority = command.params.low_priority;
        this.roomLowPriorities.set(roomId, low_priority);
        this.patchRoom(roomId, { low_priority });
        return { type: 'RoomLowPrioritySet', payload: { room_id: roomId, low_priority } };
      }

      case 'SetRoomMarkedUnread': {
        const roomId = command.params.room_id;
        const unread = command.params.unread;
        this.patchRoom(roomId, { marked_unread: unread });
        return { type: 'RoomMarkedUnreadSet', payload: { room_id: roomId, unread } };
      }

      case 'SaveDraft': {
        const roomId = command.params.room_id;
        const { text, reply_to, edit_of } = command.params;
        if (text.trim().length === 0 && !reply_to && !edit_of) {
          this.draftsByRoom.delete(roomId);
        } else {
          this.draftsByRoom.set(roomId, { text, reply_to, edit_of });
        }
        return { type: 'DraftSaved', payload: { room_id: roomId } };
      }

      case 'GetDraft': {
        const roomId = command.params.room_id;
        const draft = this.draftsByRoom.get(roomId);
        const dto = draft
          ? { text: draft.text, reply_to: draft.reply_to ?? null, edit_of: draft.edit_of ?? null }
          : null;
        return { type: 'Draft', payload: { room_id: roomId, draft: dto } };
      }

      case 'ListDrafts': {
        const items = Array.from(this.draftsByRoom.entries()).map(([room_id, { text }]) => ({
          room_id,
          text: text.slice(0, 80),
        }));
        return { type: 'Drafts', payload: { items } };
      }

      case 'DeleteDevices': {
        const deleted = command.params.device_ids;
        return { type: 'DevicesDeleted', payload: { deleted } };
      }

      case 'KickUser': {
        const { room_id, user_id } = command.params;
        return { type: 'UserKicked', payload: { room_id, user_id } };
      }

      case 'BanUser': {
        const { room_id, user_id } = command.params;
        return { type: 'UserBanned', payload: { room_id, user_id } };
      }

      case 'UnbanUser': {
        const { room_id, user_id } = command.params;
        return { type: 'UserUnbanned', payload: { room_id, user_id } };
      }

      case 'SetPowerLevel': {
        const { room_id, user_id, level } = command.params;
        return { type: 'PowerLevelSet', payload: { room_id, user_id, level } };
      }

      case 'IgnoreUser': {
        const { user_id } = command.params;
        return { type: 'UserIgnored', payload: { user_id } };
      }

      case 'UnignoreUser': {
        const { user_id } = command.params;
        return { type: 'UserUnignored', payload: { user_id } };
      }

      case 'ListIgnoredUsers':
        return { type: 'IgnoredUsers', payload: { user_ids: [] } };

      case 'ReportEvent': {
        const { room_id, event_id } = command.params;
        return { type: 'EventReported', payload: { room_id, event_id } };
      }

      case 'SearchAllMessages': {
        const hits = [
          {
            room_id: '!example:localhost',
            room_name: 'Example Room',
            event_id: '$example_event',
            sender: '@alice:localhost',
            sender_display_name: 'Alice',
            timestamp_millis: Date.now(),
            body: `Message containing "${command.params.query}"`,
          },
        ];
        return { type: 'AllMessagesFound', payload: { hits, next_batch: null } };
      }

      case 'RefreshSync': {
        return { type: 'SyncRefreshed' };
      }

      default: {
        const _exhaustive: never = command;
        throw new Error(`Unhandled command: ${JSON.stringify(_exhaustive)}`);
      }
    }
  }
}
