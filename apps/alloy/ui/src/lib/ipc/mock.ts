import type {
  EngineCommand,
  EngineResponse,
  EngineEvent,
  RoomSummary,
  TimelineItemDto,
  Account,
  SasEmoji,
  VectorDiffDto,
} from '../generated/index.js';
import type { Transport, EventHandler, Unsubscribe } from './transport.js';

const MOCK_ACCOUNT: Account = {
  id: 'acc_alice',
  user_id: '@alice:alloy.chat',
  display_name: 'Alice',
  avatar_url: null,
  homeserver_url: 'https://matrix.alloy.chat',
};

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

  constructor() {
    this.seedTimelines();
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
      },
    ];

    this.timelineByRoom.set('!general:alloy.chat', generalItems);
    this.timelineByRoom.set('!dev:alloy.chat', []);
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

  async invoke(command: EngineCommand): Promise<EngineResponse> {
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
        return {
          type: 'OidcUrl',
          payload: {
            redirect_url: `${command.params.homeserver_url}/_matrix/client/v3/auth/oidc/fallback`,
          },
        };

      case 'CompleteOidcLogin':
        return {
          type: 'LoggedIn',
          payload: { account: MOCK_ACCOUNT },
        };

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

      case 'PaginateBackwards': {
        const roomId = command.params.room_id;
        const currentItems = this.timelineByRoom.get(roomId) ?? [];
        const olderItem: TimelineItemDto = {
          internal_id: `older_${Date.now()}`,
          event_id: `$hist_${Date.now()}:alloy.chat`,
          sender: '@charlie:alloy.chat',
          timestamp_millis: Date.now() - 7200000,
          content: {
            kind: 'TextMessage',
            data: {
              body: 'Historical message prepended via backward pagination.',
              formatted_body: null,
              html_sanitized: true,
            },
          },
          send_state: { status: 'Sent', data: { event_id: `$hist_${Date.now()}:alloy.chat` } },
          is_own: false,
          reply_to: null,
          thread_summary: null,
          read_receipts: null,
        };

        currentItems.unshift(olderItem);
        this.timelineByRoom.set(roomId, currentItems);

        for (const [subId, rId] of this.timelineSubscriptions.entries()) {
          if (rId === roomId) {
            this.emit({
              type: 'TimelineDiff',
              payload: {
                subscription_id: subId,
                room_id: roomId,
                diff: { type: 'PushFront', content: { value: olderItem } },
              },
            });
          }
        }

        return { type: 'Unit' };
      }

      case 'SendText': {
        const roomId = command.params.room_id;
        const txnId = command.params.txn_id ?? `txn_${Date.now()}`;
        const internalId = `local_${txnId}`;

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
          reply_to: null,
          thread_summary: null,
          read_receipts: null,
        };

        const currentItems = this.timelineByRoom.get(roomId) ?? [];
        currentItems.push(localEcho);
        this.timelineByRoom.set(roomId, currentItems);

        // Emit local echo PushBack
        for (const [subId, rId] of this.timelineSubscriptions.entries()) {
          if (rId === roomId) {
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

      case 'MarkRead':
        return {
          type: 'ReadMarked',
          payload: { room_id: command.params.room_id },
        };

      case 'SetTyping':
        return {
          type: 'TypingStatusSet',
          payload: { room_id: command.params.room_id },
        };

      case 'GetMedia':
        return {
          type: 'MediaRetrieved',
          payload: { local_path: '/tmp/alloy-mock-cache/media.png' },
        };

      case 'RequestSelfVerification':
        return {
          type: 'VerificationRequested',
          payload: { user_id: MOCK_ACCOUNT.user_id, device_id: 'DEVICE_MOCK' },
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

      case 'SubscribeSpaces': {
        const subId = `sub_spaces_${++this.subCounter}`;
        return {
          type: 'SpacesSubscribed',
          payload: { subscription_id: subId },
        };
      }

      case 'SubscribeSpaceChildren': {
        const subId = `sub_space_child_${++this.subCounter}`;
        return {
          type: 'SpaceChildrenSubscribed',
          payload: { subscription_id: subId },
        };
      }

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

      default: {
        const _exhaustive: never = command;
        throw new Error(`Unhandled command: ${JSON.stringify(_exhaustive)}`);
      }
    }
  }
}
