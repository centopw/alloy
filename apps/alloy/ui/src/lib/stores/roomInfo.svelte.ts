import { client } from '../ipc/index.js';
import type { EngineResponse, RoomDetails } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';

interface RoomInfoEntry {
  details: RoomDetails | null;
  note: string;
  /** True once details and note were read successfully. */
  loaded: boolean;
  /** Number of writes still in flight for the room. */
  writes: number;
  error: string | null;
}

/**
 * Details, private note and edit actions of the room info drawer, cached per room.
 * Failures are stored and readable through `error(roomId)`; the methods never reject.
 */
export class RoomInfoStore {
  private rooms = $state<Record<string, RoomInfoEntry>>({});
  private inflight = new Map<string, Promise<void>>();

  details(roomId: string): RoomDetails | null {
    return this.rooms[roomId]?.details ?? null;
  }

  note(roomId: string): string {
    return this.rooms[roomId]?.note ?? '';
  }

  saving(roomId: string): boolean {
    return (this.rooms[roomId]?.writes ?? 0) > 0;
  }

  error(roomId: string): string | null {
    return this.rooms[roomId]?.error ?? null;
  }

  /** Reads details and note once per room; concurrent calls share the request. */
  load(roomId: string): Promise<void> {
    if (this.rooms[roomId]?.loaded) return Promise.resolve();
    const pending = this.inflight.get(roomId);
    if (pending) return pending;
    const p: Promise<void> = this.fetchInfo(roomId).finally(() => {
      if (this.inflight.get(roomId) === p) this.inflight.delete(roomId);
    });
    this.inflight.set(roomId, p);
    return p;
  }

  rename(roomId: string, name: string): Promise<void> {
    return this.write(roomId, () =>
      client.invoke({ action: 'SetRoomName', params: { room_id: roomId, name } })
    );
  }

  setTopic(roomId: string, topic: string): Promise<void> {
    return this.write(roomId, () =>
      client.invoke({ action: 'SetRoomTopic', params: { room_id: roomId, topic } })
    );
  }

  /** `path` is a picked file, or `null` to remove the avatar. */
  setAvatar(roomId: string, path: string | null): Promise<void> {
    return this.write(roomId, () =>
      client.invoke({ action: 'SetRoomAvatar', params: { room_id: roomId, file_path: path } })
    );
  }

  /** Saves the private note for the room. */
  saveNote(roomId: string, note: string): Promise<void> {
    return this.write(roomId, () =>
      client.invoke({ action: 'SetRoomNote', params: { room_id: roomId, note } })
    );
  }

  private room(roomId: string): RoomInfoEntry {
    if (!this.rooms[roomId]) {
      this.rooms[roomId] = { details: null, note: '', loaded: false, writes: 0, error: null };
    }
    return this.rooms[roomId];
  }

  private async fetchInfo(roomId: string): Promise<void> {
    try {
      const [detailsRes, noteRes] = await Promise.all([
        client.invoke({ action: 'GetRoomDetails', params: { room_id: roomId } }),
        client.invoke({ action: 'GetRoomNote', params: { room_id: roomId } }),
      ]);
      const details = expect(detailsRes, 'RoomDetailsRetrieved').payload.details;
      const note = expect(noteRes, 'RoomNote').payload.note;
      const room = this.room(roomId);
      room.details = details;
      room.note = note;
      room.loaded = true;
      room.error = null;
    } catch (err) {
      this.room(roomId).error = errorMessage(err);
    }
  }

  /** Runs one engine write, then re-reads details and note. Failures are stored, not thrown. */
  private async write(roomId: string, action: () => Promise<EngineResponse>): Promise<void> {
    this.room(roomId).writes += 1;
    this.room(roomId).error = null;
    try {
      expect(await action(), 'Unit');
      await this.fetchInfo(roomId);
    } catch (err) {
      this.room(roomId).error = errorMessage(err);
    } finally {
      this.room(roomId).writes -= 1;
    }
  }
}

export const roomInfo = new RoomInfoStore();
