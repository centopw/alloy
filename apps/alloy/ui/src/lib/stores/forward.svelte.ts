import { client } from '../ipc/index.js';
import { errorMessage, expect } from './errors.js';

export type ForwardState = 'sending' | 'sent' | { error: string };

/** The messages being forwarded and, per target room, how sending went. The dialog stays open so one message can go to many rooms. */
class ForwardStore {
  open = $state(false);
  roomId = $state<string | null>(null);
  eventIds = $state<string[]>([]);
  results = $state<Record<string, ForwardState>>({});

  begin(roomId: string, eventIds: string[]) {
    if (eventIds.length === 0) return;
    this.roomId = roomId;
    this.eventIds = eventIds;
    this.results = {};
    this.open = true;
  }

  close() {
    this.open = false;
    this.roomId = null;
    this.eventIds = [];
    this.results = {};
  }

  async sendTo(targetId: string) {
    const source = this.roomId;
    if (!source || this.results[targetId] === 'sending' || this.results[targetId] === 'sent') return;
    this.results[targetId] = 'sending';
    try {
      const res = expect(await client.forwardEvents(source, this.eventIds, [targetId]), 'EventsForwarded');
      const failure = res.payload.failed.find((f) => f.room_id === targetId);
      this.results[targetId] = failure ? { error: failure.error } : 'sent';
    } catch (e) {
      this.results[targetId] = { error: errorMessage(e) };
    }
  }
}

export const forwardStore = new ForwardStore();
