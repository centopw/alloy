import type {
  AccountId,
  EngineCommand,
  EngineResponse,
  EventId,
  LoginId,
  LoginInputDto,
  MediaSource,
  Network,
  RoomId,
  RoomMediaKind,
  SubscriptionId,
  TagColour,
  UserId,
  VectorDiffDto,
} from '../generated/index.js';
import type { EventHandler, Transport, Unsubscribe } from './transport.js';

/**
 * Apply an eyeball-im VectorDiffDto operation to an array in-place.
 * Compatible with Svelte 5 `$state([])` reactive proxies.
 * Handles all 11 diff variants: Append, Clear, PushFront, PushBack,
 * PopFront, PopBack, Insert, Set, Remove, Truncate, Reset.
 */
export function applyDiff<T>(target: T[], diff: VectorDiffDto<T>): T[] {
  switch (diff.type) {
    case 'Append':
      target.push(...diff.content.values);
      break;
    case 'Clear':
      target.length = 0;
      break;
    case 'PushFront':
      target.unshift(diff.content.value);
      break;
    case 'PushBack':
      target.push(diff.content.value);
      break;
    case 'PopFront':
      target.shift();
      break;
    case 'PopBack':
      target.pop();
      break;
    case 'Insert':
      target.splice(diff.content.index, 0, diff.content.value);
      break;
    case 'Set':
      if (diff.content.index >= 0 && diff.content.index < target.length) {
        target[diff.content.index] = diff.content.value;
      }
      break;
    case 'Remove':
      if (diff.content.index >= 0 && diff.content.index < target.length) {
        target.splice(diff.content.index, 1);
      }
      break;
    case 'Truncate':
      if (diff.content.length < target.length) {
        target.length = diff.content.length;
      }
      break;
    case 'Reset':
      target.splice(0, target.length, ...diff.content.values);
      break;
  }
  return target;
}

/**
 * High-level typed client wrapping the low-level Transport.
 *
 * One method per v2 `EngineCommand`, named after the action in camelCase, with the command's
 * parameters as positional arguments in protocol order. Nullable parameters default to `null`, so
 * a caller that does not need them keeps the call short.
 */
export class AlloyClient {
  constructor(private transport: Transport) {}

  /**
   * Subscribe to events from the backend.
   */
  subscribe(handler: EventHandler): Unsubscribe {
    return this.transport.subscribe(handler);
  }

  /**
   * Raw command invoke.
   */
  invoke(cmd: EngineCommand): Promise<EngineResponse> {
    return this.transport.invoke(cmd);
  }

  // === Accounts and login ===

  async listAccounts() {
    return this.transport.invoke({ action: 'ListAccounts' });
  }

  async listNetworks() {
    return this.transport.invoke({ action: 'ListNetworks' });
  }

  /** Starts a login attempt and returns its first step. */
  async beginLogin(network: Network) {
    return this.transport.invoke({ action: 'BeginLogin', params: { network } });
  }

  /** Answers the step the engine last returned. */
  async submitLogin(login_id: LoginId, input: LoginInputDto) {
    return this.transport.invoke({ action: 'SubmitLogin', params: { login_id, input } });
  }

  /** Abandons an attempt; nothing is stored. */
  async cancelLogin(login_id: LoginId) {
    return this.transport.invoke({ action: 'CancelLogin', params: { login_id } });
  }

  /** Removes the account and its local data after revoking the session on the network. */
  async logout(account_id: AccountId) {
    return this.transport.invoke({ action: 'Logout', params: { account_id } });
  }

  // === Chats ===

  /** Starts the room-list stream; `filter` narrows the projection by chat name. */
  async subscribeRoomList(filter: string | null = null) {
    return this.transport.invoke({ action: 'SubscribeRoomList', params: { filter } });
  }

  /** Aborts any subscription (room list or timeline). */
  async unsubscribe(subscription_id: SubscriptionId) {
    return this.transport.invoke({ action: 'Unsubscribe', params: { subscription_id } });
  }

  async getRoomMembers(room_id: RoomId) {
    return this.transport.invoke({ action: 'GetRoomMembers', params: { room_id } });
  }

  async getRoomDetails(room_id: RoomId) {
    return this.transport.invoke({ action: 'GetRoomDetails', params: { room_id } });
  }

  async getUserProfile(account_id: AccountId, user_id: UserId) {
    return this.transport.invoke({ action: 'GetUserProfile', params: { account_id, user_id } });
  }

  /** The private note the user keeps about a chat; local, never sent to the network. */
  async getRoomNote(room_id: RoomId) {
    return this.transport.invoke({ action: 'GetRoomNote', params: { room_id } });
  }

  /** Stores the private note; an empty note deletes it. */
  async setRoomNote(room_id: RoomId, note: string) {
    return this.transport.invoke({ action: 'SetRoomNote', params: { room_id, note } });
  }

  async getTagCatalog() {
    return this.transport.invoke({ action: 'GetTagCatalog' });
  }

  async createRoomTag(name: string, colour: TagColour) {
    return this.transport.invoke({ action: 'CreateRoomTag', params: { name, colour } });
  }

  /** Renames and/or recolours one tag; at least one of `name`/`colour` must be set. */
  async renameRoomTag(id: string, name: string | null = null, colour: TagColour | null = null) {
    return this.transport.invoke({ action: 'RenameRoomTag', params: { id, name, colour } });
  }

  /** Sets the catalogue's display order; `ids` must list every tag exactly once. */
  async reorderRoomTags(ids: string[]) {
    return this.transport.invoke({ action: 'ReorderRoomTags', params: { ids } });
  }

  async setRoomTag(room_id: RoomId, tag_id: string, enabled: boolean) {
    return this.transport.invoke({ action: 'SetRoomTag', params: { room_id, tag_id, enabled } });
  }

  async deleteRoomTag(tag_id: string) {
    return this.transport.invoke({ action: 'DeleteRoomTag', params: { tag_id } });
  }

  // === Chat flags ===

  async markRoomRead(room_id: RoomId) {
    return this.transport.invoke({ action: 'MarkRoomRead', params: { room_id } });
  }

  async setRoomPinned(room_id: RoomId, pinned: boolean) {
    return this.transport.invoke({ action: 'SetRoomPinned', params: { room_id, pinned } });
  }

  async setRoomArchived(room_id: RoomId, archived: boolean) {
    return this.transport.invoke({ action: 'SetRoomArchived', params: { room_id, archived } });
  }

  async setRoomMarkedUnread(room_id: RoomId, unread: boolean) {
    return this.transport.invoke({ action: 'SetRoomMarkedUnread', params: { room_id, unread } });
  }

  /** Mutes until `muted_until` (ms since the Unix epoch); `null` unmutes. */
  async setRoomMuted(room_id: RoomId, muted_until: number | null = null) {
    return this.transport.invoke({ action: 'SetRoomMuted', params: { room_id, muted_until } });
  }

  // === Local search and shared media ===

  /** Shared media, files or links of a chat, newest first; `before` is the previous `next` cursor. */
  async getRoomMedia(
    room_id: RoomId,
    kind: RoomMediaKind,
    before: string | null = null,
    limit = 50
  ) {
    return this.transport.invoke({ action: 'GetRoomMedia', params: { room_id, kind, before, limit } });
  }

  async searchRoomMessages(room_id: RoomId, query: string, limit = 50) {
    return this.transport.invoke({ action: 'SearchRoomMessages', params: { room_id, query, limit } });
  }

  async searchAllMessages(query: string, limit: number | null = null) {
    return this.transport.invoke({ action: 'SearchAllMessages', params: { query, limit } });
  }

  async clearMediaCache() {
    return this.transport.invoke({ action: 'ClearMediaCache' });
  }

  /** Reconnects every account's connector; live updates resume as they catch up. */
  async refreshSync() {
    return this.transport.invoke({ action: 'RefreshSync' });
  }

  // === Timeline ===

  async subscribeTimeline(room_id: RoomId) {
    return this.transport.invoke({ action: 'SubscribeTimeline', params: { room_id } });
  }

  /** Aborts the timeline subscription of one chat and drops its cached timeline. */
  async closeTimeline(room_id: RoomId) {
    return this.transport.invoke({ action: 'CloseTimeline', params: { room_id } });
  }

  async paginateBackwards(room_id: RoomId, count: number) {
    return this.transport.invoke({ action: 'PaginateBackwards', params: { room_id, count } });
  }

  /** Queues a text message; the local echo arrives through `TimelineDiff`. */
  async sendText(
    room_id: RoomId,
    body: string,
    reply_to: EventId | null = null,
    txn_id: string | null = null
  ) {
    return this.transport.invoke({ action: 'SendText', params: { room_id, body, reply_to, txn_id } });
  }

  async editMessage(room_id: RoomId, event_id: EventId, body: string) {
    return this.transport.invoke({ action: 'EditMessage', params: { room_id, event_id, body } });
  }

  async deleteMessage(room_id: RoomId, event_id: EventId, for_everyone: boolean) {
    return this.transport.invoke({
      action: 'DeleteMessage',
      params: { room_id, event_id, for_everyone },
    });
  }

  /** Adds the reaction `key`, or removes it when the user already reacted with it. */
  async toggleReaction(room_id: RoomId, event_id: EventId, key: string) {
    return this.transport.invoke({ action: 'ToggleReaction', params: { room_id, event_id, key } });
  }

  /** Forwards messages to one or more target chats, dropping replies and reactions. */
  async forwardEvents(room_id: RoomId, event_ids: EventId[], targets: RoomId[]) {
    return this.transport.invoke({ action: 'ForwardEvents', params: { room_id, event_ids, targets } });
  }

  /** Saves a draft; empty text without a reply clears it. */
  async saveDraft(room_id: RoomId, text: string, reply_to: EventId | null = null) {
    return this.transport.invoke({ action: 'SaveDraft', params: { room_id, text, reply_to } });
  }

  async getDraft(room_id: RoomId) {
    return this.transport.invoke({ action: 'GetDraft', params: { room_id } });
  }

  async listDrafts() {
    return this.transport.invoke({ action: 'ListDrafts' });
  }

  async retrySend(room_id: RoomId, txn_id: string) {
    return this.transport.invoke({ action: 'RetrySend', params: { room_id, txn_id } });
  }

  async cancelSend(room_id: RoomId, txn_id: string) {
    return this.transport.invoke({ action: 'CancelSend', params: { room_id, txn_id } });
  }

  async sendAttachment(
    room_id: RoomId,
    file_path: string,
    mime_type: string | null = null,
    body: string | null = null,
    txn_id: string | null = null
  ) {
    return this.transport.invoke({
      action: 'SendAttachment',
      params: { room_id, file_path, mime_type, body, txn_id },
    });
  }

  /** Sends a voice note; the file stays the user's, the engine copies it into the store. */
  async sendVoice(room_id: RoomId, file_path: string, duration_ms: number, txn_id: string | null = null) {
    return this.transport.invoke({
      action: 'SendVoice',
      params: { room_id, file_path, duration_ms, txn_id },
    });
  }

  /** Reports that the user read the chat up to `event_id`. */
  async markRead(room_id: RoomId, event_id: EventId) {
    return this.transport.invoke({ action: 'MarkRead', params: { room_id, event_id } });
  }

  async setTyping(room_id: RoomId, typing: boolean) {
    return this.transport.invoke({ action: 'SetTyping', params: { room_id, typing } });
  }

  // === Media ===

  /** Fetches a stored media object into the local file cache and returns its path. */
  async getMedia(source: MediaSource, kind: 'thumbnail' | 'file', mime_type: string | null = null) {
    return this.transport.invoke({ action: 'GetMedia', params: { source, kind, mime_type } });
  }

  // === Settings and themes ===

  async listThemes() {
    return this.transport.invoke({ action: 'ListThemes' });
  }

  async getTheme(id: string) {
    return this.transport.invoke({ action: 'GetTheme', params: { id } });
  }

  async setActiveTheme(id: string) {
    return this.transport.invoke({ action: 'SetActiveTheme', params: { id } });
  }

  async getSettings() {
    return this.transport.invoke({ action: 'GetSettings' });
  }

  /** Stores one setting; `value_json` is the JSON text the engine persists. */
  async setSetting(key: string, value_json: string) {
    return this.transport.invoke({ action: 'SetSetting', params: { key, value_json } });
  }
}
