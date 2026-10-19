import { client } from '../ipc/index.js';
import type { GlobalSearchHit } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';
import { timelineStore } from './timeline.svelte.js';

/** Queries shorter than this are not sent to the server. */
export const GLOBAL_SEARCH_MIN_CHARS = 3;
const DEBOUNCE_MS = 400;

/** Whether `query` is long enough to search all rooms for. */
export function isSearchable(query: string): boolean {
  return query.trim().length >= GLOBAL_SEARCH_MIN_CHARS;
}

/** Search across every unencrypted room, shown under the room list while the user types in the sidebar search. */
class GlobalSearchStore {
  hits = $state<GlobalSearchHit[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  nextBatch = $state<string | null>(null);
  /** The query the shown hits belong to. */
  query = $state('');

  private timer: ReturnType<typeof setTimeout> | undefined;
  private seq = 0;

  /** Starts a search for `query` after the user pauses typing; a short query clears the results. */
  schedule(query: string) {
    clearTimeout(this.timer);
    this.seq++;
    if (!isSearchable(query)) {
      this.reset();
      return;
    }
    this.loading = true;
    const seq = this.seq;
    this.timer = setTimeout(() => void this.run(query.trim(), null, seq), DEBOUNCE_MS);
  }

  async more() {
    if (!this.nextBatch || this.loading) return;
    this.loading = true;
    await this.run(this.query, this.nextBatch, this.seq);
  }

  private async run(query: string, batch: string | null, seq: number) {
    try {
      const res = expect(await client.searchAllMessages(query, batch), 'AllMessagesFound');
      if (seq !== this.seq) return;
      this.hits = batch ? [...this.hits, ...res.payload.hits] : res.payload.hits;
      this.nextBatch = res.payload.next_batch;
      this.query = query;
      this.error = null;
    } catch (e) {
      if (seq === this.seq) this.error = errorMessage(e);
    } finally {
      if (seq === this.seq) this.loading = false;
    }
  }

  /** Opens the room of a hit and scrolls to the message once its timeline is ready. */
  async open(hit: GlobalSearchHit, openRoom: (roomId: string) => void) {
    openRoom(hit.room_id);
    for (let i = 0; i < 80; i++) {
      if (timelineStore.roomId === hit.room_id && !timelineStore.loading) {
        await timelineStore.jumpToEvent(hit.event_id);
        return;
      }
      await new Promise((r) => setTimeout(r, 50));
    }
  }

  reset() {
    clearTimeout(this.timer);
    this.seq++;
    this.hits = [];
    this.nextBatch = null;
    this.query = '';
    this.loading = false;
    this.error = null;
  }
}

export const globalSearch = new GlobalSearchStore();
