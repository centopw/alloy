import { client } from '../ipc/index.js';
import type { NotificationLevel } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';

interface RoomNotifyEntry {
  /** Effective level the engine resolves for the room (its own override, or the account default). */
  level: NotificationLevel;
  /** True when the room follows the account default rather than an override. */
  isDefault: boolean;
  /** Set once a read succeeded; false while only a failed write has touched the entry. */
  loaded: boolean;
  /** Number of writes still in flight for the room. */
  writes: number;
  error: string | null;
}

/**
 * Per-room notification levels, cached so the sidebar and notifier can read them synchronously.
 * Reads and writes go through the engine; failures are stored and readable through `error(roomId)`,
 * so the methods never reject.
 *
 * `levelOf` is the effective level (what the engine resolves), or `null` until the room was read.
 */
export class RoomNotifyStore {
  private rooms = $state<Record<string, RoomNotifyEntry>>({});
  private inflight = new Map<string, Promise<void>>();

  /** Effective level of the room, or `null` when it has not been read (or the read failed). */
  levelOf(roomId: string): NotificationLevel | null {
    const e = this.rooms[roomId];
    return e?.loaded ? e.level : null;
  }

  /** Whether the room follows the account default, or `null` when unknown. */
  isDefault(roomId: string): boolean | null {
    const e = this.rooms[roomId];
    return e?.loaded ? e.isDefault : null;
  }

  /** Whether the room is muted (effective level `Mute`). False while unknown. */
  isMuted(roomId: string): boolean {
    return this.levelOf(roomId) === 'Mute';
  }

  /** True while a write for the room is still in flight. */
  saving(roomId: string): boolean {
    return (this.rooms[roomId]?.writes ?? 0) > 0;
  }

  error(roomId: string): string | null {
    return this.rooms[roomId]?.error ?? null;
  }

  /** Reads the room's level once; concurrent calls share the request. */
  load(roomId: string): Promise<void> {
    if (this.rooms[roomId]?.loaded) return Promise.resolve();
    const pending = this.inflight.get(roomId);
    if (pending) return pending;
    const p: Promise<void> = this.fetch(roomId).finally(() => {
      if (this.inflight.get(roomId) === p) this.inflight.delete(roomId);
    });
    this.inflight.set(roomId, p);
    return p;
  }

  /** Sets the room's override, or restores the account default with `null`, then re-reads it. */
  async set(roomId: string, level: NotificationLevel | null): Promise<void> {
    const entry = this.room(roomId);
    entry.writes += 1;
    entry.error = null;
    try {
      expect(
        await client.invoke({ action: 'SetRoomNotificationLevel', params: { room_id: roomId, level } }),
        'Unit'
      );
      await this.fetch(roomId);
    } catch (err) {
      this.room(roomId).error = errorMessage(err);
    } finally {
      this.room(roomId).writes -= 1;
    }
  }

  /** Drops every cached level (on sign-out or when the room list restarts). */
  clear(): void {
    this.rooms = {};
    this.inflight.clear();
  }

  private room(roomId: string): RoomNotifyEntry {
    if (!this.rooms[roomId]) {
      this.rooms[roomId] = { level: 'All', isDefault: true, loaded: false, writes: 0, error: null };
    }
    return this.rooms[roomId];
  }

  private async fetch(roomId: string): Promise<void> {
    try {
      const res = await client.invoke({ action: 'GetRoomNotificationLevel', params: { room_id: roomId } });
      const payload = expect(res, 'RoomNotificationLevel').payload;
      const entry = this.room(roomId);
      entry.level = payload.level;
      entry.isDefault = payload.is_default;
      entry.loaded = true;
      entry.error = null;
    } catch (err) {
      this.room(roomId).error = errorMessage(err);
    }
  }
}

export const roomNotify = new RoomNotifyStore();
