import type {
  EngineCommand,
  EngineResponse,
  EngineEvent,
  VectorDiffDto,
  RoomId,
  EventId,
  UserId,
  AccountId,
  SubscriptionId,
  MediaSource,
} from '../generated/index.js';
import type { Transport, EventHandler, Unsubscribe } from './transport.js';

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

  // === Auth ===

  async discoverHomeserver(input: string) {
    return this.transport.invoke({
      action: 'DiscoverHomeserver',
      params: { input },
    });
  }

  async loginPassword(homeserver_url: string, username: string, password: string) {
    return this.transport.invoke({
      action: 'LoginPassword',
      params: { homeserver_url, username, password },
    });
  }

  async startOidcLogin(homeserver_url: string) {
    return this.transport.invoke({
      action: 'StartOidcLogin',
      params: { homeserver_url },
    });
  }

  async completeOidcLogin(redirect_url: string) {
    return this.transport.invoke({
      action: 'CompleteOidcLogin',
      params: { redirect_url },
    });
  }

  async restoreSession(account_id: AccountId) {
    return this.transport.invoke({
      action: 'RestoreSession',
      params: { account_id },
    });
  }

  async logout(account_id: AccountId) {
    return this.transport.invoke({
      action: 'Logout',
      params: { account_id },
    });
  }

  async listAccounts() {
    return this.transport.invoke({
      action: 'ListAccounts',
    });
  }

  // === Rooms ===

  async subscribeRoomList(filter?: string | null) {
    return this.transport.invoke({
      action: 'SubscribeRoomList',
      params: { filter: filter ?? null },
    });
  }

  async unsubscribe(subscription_id: SubscriptionId) {
    return this.transport.invoke({
      action: 'Unsubscribe',
      params: { subscription_id },
    });
  }

  async joinRoom(room_id_or_alias: string) {
    return this.transport.invoke({
      action: 'JoinRoom',
      params: { room_id_or_alias },
    });
  }

  async leaveRoom(room_id: RoomId) {
    return this.transport.invoke({
      action: 'LeaveRoom',
      params: { room_id },
    });
  }

  async createRoom(name: string, encrypted: boolean, invite?: UserId[] | null) {
    return this.transport.invoke({
      action: 'CreateRoom',
      params: { name, encrypted, invite: invite ?? null },
    });
  }

  // === Timeline ===

  async subscribeTimeline(room_id: RoomId) {
    return this.transport.invoke({
      action: 'SubscribeTimeline',
      params: { room_id },
    });
  }

  /** Streams one thread (root plus replies) as diffs under the returned subscription id. */
  async subscribeThread(room_id: RoomId, thread_root_event_id: EventId) {
    return this.transport.invoke({
      action: 'SubscribeThread',
      params: { room_id, thread_root_event_id },
    });
  }

  async paginateBackwards(room_id: RoomId, count: number) {
    return this.transport.invoke({
      action: 'PaginateBackwards',
      params: { room_id, count },
    });
  }

  async sendText(
    room_id: RoomId,
    body: string,
    opts?: {
      markdown?: boolean | null;
      reply_to?: EventId | null;
      mentions?: UserId[] | null;
      txn_id?: string | null;
      in_thread_root?: EventId | null;
      mention_room?: boolean;
    }
  ) {
    return this.transport.invoke({
      action: 'SendText',
      params: {
        room_id,
        body,
        markdown: opts?.markdown ?? null,
        reply_to: opts?.reply_to ?? null,
        mentions: opts?.mentions ?? null,
        txn_id: opts?.txn_id ?? null,
        in_thread_root: opts?.in_thread_root ?? null,
        mention_room: opts?.mention_room ?? false,
      },
    });
  }

  async editMessage(room_id: RoomId, event_id: EventId, body: string, markdown?: boolean | null) {
    return this.transport.invoke({
      action: 'EditMessage',
      params: {
        room_id,
        event_id,
        body,
        markdown: markdown ?? null,
      },
    });
  }

  async redactEvent(room_id: RoomId, event_id: EventId, reason?: string | null) {
    return this.transport.invoke({
      action: 'RedactEvent',
      params: { room_id, event_id, reason: reason ?? null },
    });
  }

  async toggleReaction(room_id: RoomId, event_id: EventId, key: string) {
    return this.transport.invoke({
      action: 'ToggleReaction',
      params: { room_id, event_id, key },
    });
  }

  async sendPoll(
    room_id: RoomId,
    question: string,
    answers: string[],
    disclosed: boolean,
    max_selections: number
  ) {
    return this.transport.invoke({
      action: 'SendPoll',
      params: { room_id, question, answers, disclosed, max_selections },
    });
  }

  async respondToPoll(room_id: RoomId, poll_event_id: EventId, answer_ids: string[]) {
    return this.transport.invoke({
      action: 'RespondToPoll',
      params: { room_id, poll_event_id, answer_ids },
    });
  }

  async endPoll(room_id: RoomId, poll_event_id: EventId) {
    return this.transport.invoke({
      action: 'EndPoll',
      params: { room_id, poll_event_id },
    });
  }

  async retrySend(room_id: RoomId, txn_id: string) {
    return this.transport.invoke({
      action: 'RetrySend',
      params: { room_id, txn_id },
    });
  }

  async cancelSend(room_id: RoomId, txn_id: string) {
    return this.transport.invoke({
      action: 'CancelSend',
      params: { room_id, txn_id },
    });
  }

  async sendAttachment(
    room_id: RoomId,
    file_path: string,
    opts?: { mime_type?: string | null; txn_id?: string | null }
  ) {
    return this.transport.invoke({
      action: 'SendAttachment',
      params: {
        room_id,
        file_path,
        mime_type: opts?.mime_type ?? null,
        txn_id: opts?.txn_id ?? null,
      },
    });
  }

  async sendVoice(
    room_id: RoomId,
    file_path: string,
    duration_ms: number,
    waveform: number[],
    txn_id?: string | null
  ) {
    return this.transport.invoke({
      action: 'SendVoice',
      params: {
        room_id,
        file_path,
        duration_ms,
        waveform,
        txn_id: txn_id ?? null,
      },
    });
  }

  async sendLocation(
    room_id: RoomId,
    latitude: number,
    longitude: number,
    opts?: {
      accuracy_m?: number | null;
      description?: string | null;
    }
  ) {
    return this.transport.invoke({
      action: 'SendLocation',
      params: {
        room_id,
        latitude,
        longitude,
        accuracy_m: opts?.accuracy_m ?? null,
        description: opts?.description ?? null,
      },
    });
  }

  async startLiveLocation(room_id: RoomId, duration_ms: number) {
    return this.transport.invoke({
      action: 'StartLiveLocation',
      params: { room_id, duration_ms },
    });
  }

  async sendLocationBeacon(room_id: RoomId, geo_uri: string) {
    return this.transport.invoke({
      action: 'SendLocationBeacon',
      params: { room_id, geo_uri },
    });
  }

  async stopLiveLocation(room_id: RoomId) {
    return this.transport.invoke({
      action: 'StopLiveLocation',
      params: { room_id },
    });
  }

  async setEventPinned(room_id: RoomId, event_id: EventId, pinned: boolean) {
    return this.transport.invoke({
      action: 'SetEventPinned',
      params: { room_id, event_id, pinned },
    });
  }

  async getPinnedEvents(room_id: RoomId) {
    return this.transport.invoke({
      action: 'GetPinnedEvents',
      params: { room_id },
    });
  }

  async forwardEvents(room_id: RoomId, event_ids: EventId[], targets: RoomId[]) {
    return this.transport.invoke({
      action: 'ForwardEvents',
      params: { room_id, event_ids, targets },
    });
  }

  async setRoomFavourite(room_id: RoomId, favourite: boolean) {
    return this.transport.invoke({
      action: 'SetRoomFavourite',
      params: { room_id, favourite },
    });
  }

  async setRoomLowPriority(room_id: RoomId, low_priority: boolean) {
    return this.transport.invoke({
      action: 'SetRoomLowPriority',
      params: { room_id, low_priority },
    });
  }

  async setRoomMarkedUnread(room_id: RoomId, unread: boolean) {
    return this.transport.invoke({
      action: 'SetRoomMarkedUnread',
      params: { room_id, unread },
    });
  }

  async saveDraft(
    room_id: RoomId,
    text: string,
    opts?: {
      reply_to?: EventId | null;
      edit_of?: EventId | null;
    }
  ) {
    return this.transport.invoke({
      action: 'SaveDraft',
      params: {
        room_id,
        text,
        reply_to: opts?.reply_to ?? null,
        edit_of: opts?.edit_of ?? null,
      },
    });
  }

  async getDraft(room_id: RoomId) {
    return this.transport.invoke({
      action: 'GetDraft',
      params: { room_id },
    });
  }

  async listDrafts() {
    return this.transport.invoke({
      action: 'ListDrafts',
    });
  }
  async markRead(room_id: RoomId, event_id: EventId) {
    return this.transport.invoke({
      action: 'MarkRead',
      params: { room_id, event_id },
    });
  }

  async setTyping(room_id: RoomId, typing: boolean) {
    return this.transport.invoke({
      action: 'SetTyping',
      params: { room_id, typing },
    });
  }

  // === Media ===

  async getMedia(
    source: MediaSource,
    kind: 'thumbnail' | 'file',
    opts?: { width?: number | null; height?: number | null; mime_type?: string | null }
  ) {
    return this.transport.invoke({
      action: 'GetMedia',
      params: {
        source,
        kind,
        width: opts?.width ?? null,
        height: opts?.height ?? null,
        mime_type: opts?.mime_type ?? null,
      },
    });
  }

  // === Crypto ===

  async requestSelfVerification() {
    return this.transport.invoke({
      action: 'RequestSelfVerification',
    });
  }

  async acceptVerification(user_id: UserId, device_id: string) {
    return this.transport.invoke({
      action: 'AcceptVerification',
      params: { user_id, device_id },
    });
  }

  async startSas(user_id: UserId, device_id: string) {
    return this.transport.invoke({
      action: 'StartSas',
      params: { user_id, device_id },
    });
  }

  async confirmSas(user_id: UserId, device_id: string) {
    return this.transport.invoke({
      action: 'ConfirmSas',
      params: { user_id, device_id },
    });
  }

  async cancelVerification(user_id: UserId, device_id: string) {
    return this.transport.invoke({
      action: 'CancelVerification',
      params: { user_id, device_id },
    });
  }

  async getRecoveryStatus() {
    return this.transport.invoke({
      action: 'GetRecoveryStatus',
    });
  }

  /** Returns the recovery key in the response payload; show it once. */
  async enableRecovery(password: string | null = null) {
    return this.transport.invoke({
      action: 'EnableRecovery',
      params: { password },
    });
  }

  async recoverWithKey(recovery_key: string) {
    return this.transport.invoke({
      action: 'RecoverWithKey',
      params: { recovery_key },
    });
  }

  async resetRecovery(password: string | null = null) {
    return this.transport.invoke({
      action: 'ResetRecovery',
      params: { password },
    });
  }

  // === Spaces ===

  async getSpaceParents() {
    return this.transport.invoke({
      action: 'GetSpaceParents',
    });
  }

  // === Settings & Themes ===

  async listThemes() {
    return this.transport.invoke({
      action: 'ListThemes',
    });
  }

  async getTheme(id: string) {
    return this.transport.invoke({
      action: 'GetTheme',
      params: { id },
    });
  }

  async setActiveTheme(id: string) {
    return this.transport.invoke({
      action: 'SetActiveTheme',
      params: { id },
    });
  }

  async getSettings() {
    return this.transport.invoke({
      action: 'GetSettings',
    });
  }

  async setSetting(key: string, value: unknown) {
    return this.transport.invoke({
      action: 'SetSetting',
      params: {
        key,
        value_json: JSON.stringify(value),
      },
    });
  }

  // === Account & Safety ===

  async deleteDevices(device_ids: string[], password: string | null = null) {
    return this.transport.invoke({
      action: 'DeleteDevices',
      params: { device_ids, password },
    });
  }

  async kickUser(room_id: RoomId, user_id: UserId, reason: string | null = null) {
    return this.transport.invoke({
      action: 'KickUser',
      params: { room_id, user_id, reason },
    });
  }

  async banUser(room_id: RoomId, user_id: UserId, reason: string | null = null) {
    return this.transport.invoke({
      action: 'BanUser',
      params: { room_id, user_id, reason },
    });
  }

  async unbanUser(room_id: RoomId, user_id: UserId, reason: string | null = null) {
    return this.transport.invoke({
      action: 'UnbanUser',
      params: { room_id, user_id, reason },
    });
  }

  async setPowerLevel(room_id: RoomId, user_id: UserId, level: number) {
    return this.transport.invoke({
      action: 'SetPowerLevel',
      params: { room_id, user_id, level },
    });
  }

  async ignoreUser(user_id: UserId) {
    return this.transport.invoke({
      action: 'IgnoreUser',
      params: { user_id },
    });
  }

  async unignoreUser(user_id: UserId) {
    return this.transport.invoke({
      action: 'UnignoreUser',
      params: { user_id },
    });
  }

  async listIgnoredUsers() {
    return this.transport.invoke({
      action: 'ListIgnoredUsers',
    });
  }

  async reportEvent(room_id: RoomId, event_id: EventId, reason: string | null = null) {
    return this.transport.invoke({
      action: 'ReportEvent',
      params: { room_id, event_id, reason },
    });
  }

  async searchAllMessages(query: string, next_batch: string | null = null, limit?: number) {
    return this.transport.invoke({
      action: 'SearchAllMessages',
      params: { query, next_batch, limit },
    });
  }
}
