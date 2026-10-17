import { client } from '../ipc/index.js';
import type { RoomDetails, RoomMemberDto } from '../generated/index.js';
import { expect } from './errors.js';

/** Members and details of the rooms the UI has looked at, cached per room. */
class MembersStore {
  private members = $state<Record<string, RoomMemberDto[]>>({});
  private details = $state<Record<string, RoomDetails>>({});
  private inflight = new Map<string, Promise<void>>();

  /** Cached members of a room (empty until `loadMembers` resolves). */
  membersOf(roomId: string | null): RoomMemberDto[] {
    return roomId ? (this.members[roomId] ?? []) : [];
  }

  detailsOf(roomId: string | null): RoomDetails | null {
    return roomId ? (this.details[roomId] ?? null) : null;
  }

  /** Fetch members from the engine; concurrent calls for one room share a request. */
  loadMembers(roomId: string, force = false): Promise<void> {
    if (!force && this.members[roomId]) return Promise.resolve();
    const pending = this.inflight.get(roomId);
    if (pending) return pending;
    const p = client
      .invoke({ action: 'GetRoomMembers', params: { room_id: roomId } })
      .then((res) => {
        this.members[roomId] = expect(res, 'RoomMembers').payload.members;
      })
      .finally(() => this.inflight.delete(roomId));
    this.inflight.set(roomId, p);
    return p;
  }

  async loadDetails(roomId: string): Promise<void> {
    const res = await client.invoke({ action: 'GetRoomDetails', params: { room_id: roomId } });
    this.details[roomId] = expect(res, 'RoomDetailsRetrieved').payload.details;
  }

  /** Display name for mentions and lists: display name, else the localpart. */
  static label(m: RoomMemberDto): string {
    return m.display_name?.trim() || m.user_id.replace(/^@/, '').split(':')[0];
  }

  reset() {
    this.members = {};
    this.details = {};
    this.inflight.clear();
  }
}

export const membersStore = new MembersStore();
export const memberLabel = MembersStore.label;
