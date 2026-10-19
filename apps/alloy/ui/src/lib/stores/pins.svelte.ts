import { client } from '../ipc/index.js';
import type { TimelineItemDto } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';

/** Pinned messages of the rooms the UI has looked at, oldest pin first. */
class PinsStore {
  private byRoom = $state<Record<string, TimelineItemDto[]>>({});
  private unlisten: (() => void) | null = null;
  error = $state<string | null>(null);

  itemsOf(roomId: string | null): TimelineItemDto[] {
    return roomId ? (this.byRoom[roomId] ?? []) : [];
  }

  isPinned(roomId: string | null, eventId: string | null | undefined): boolean {
    return !!eventId && this.itemsOf(roomId).some((i) => i.event_id === eventId);
  }

  /** Fetch the pinned messages of a room, and keep them fresh while the room changes. */
  async load(roomId: string): Promise<void> {
    this.watch();
    try {
      const res = expect(await client.getPinnedEvents(roomId), 'PinnedEvents');
      this.byRoom[roomId] = res.payload.items;
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  async setPinned(roomId: string, eventId: string, pinned: boolean): Promise<void> {
    try {
      expect(await client.setEventPinned(roomId, eventId, pinned), 'EventPinned');
      await this.load(roomId);
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** The engine announces pin changes made on any device; reload the room they concern. */
  private watch() {
    if (this.unlisten) return;
    this.unlisten = client.subscribe((e) => {
      if (e.type === 'PinnedEventsChanged') void this.load(e.payload.room_id);
    });
  }

  reset() {
    this.unlisten?.();
    this.unlisten = null;
    this.byRoom = {};
    this.error = null;
  }
}

export const pinsStore = new PinsStore();
