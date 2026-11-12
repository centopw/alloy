import { client } from '../ipc/index.js';
import type { GlobalSearchHit } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';
import { timelineStore } from './timeline.svelte.js';

/** Queries shorter than this are not searched for. */
export const GLOBAL_SEARCH_MIN_CHARS = 3;
/** Hits asked of the local full-text index per search. */
export const GLOBAL_SEARCH_LIMIT = 100;
const DEBOUNCE_MS = 400;

/** Whether `query` is long enough to search all chats for. */
export function isSearchable(query: string): boolean {
  return query.trim().length >= GLOBAL_SEARCH_MIN_CHARS;
}

/** Search across every stored chat, shown under the room list while the user types in its search box. */
class GlobalSearchStore {
  hits = $state<GlobalSearchHit[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
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
    this.timer = setTimeout(() => void this.run(query.trim(), seq), DEBOUNCE_MS);
  }

  private async run(query: string, seq: number) {
    try {
      const res = expect(
        await client.invoke({ action: 'SearchAllMessages', params: { query, limit: GLOBAL_SEARCH_LIMIT } }),
        'AllMessagesFound'
      );
      if (seq !== this.seq) return;
      this.hits = res.payload.hits;
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
      const { promise, resolve } = Promise.withResolvers<void>();
      setTimeout(resolve, 50);
      await promise;
    }
  }

  reset() {
    clearTimeout(this.timer);
    this.seq++;
    this.hits = [];
    this.query = '';
    this.loading = false;
    this.error = null;
  }
}

export const globalSearch = new GlobalSearchStore();
