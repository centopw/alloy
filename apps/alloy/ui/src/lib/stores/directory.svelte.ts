import { client } from '../ipc/index.js';
import type { AccountId, PublicRoomDto } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';

/** Rooms requested per directory page. */
export const DIRECTORY_PAGE_SIZE = 20;

/** Public room directory search state for the browse dialog. Only the newest search is kept. */
class DirectoryStore {
  query = $state('');
  rooms = $state<PublicRoomDto[]>([]);
  totalEstimate = $state<number | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  private nextBatch = $state<string | null>(null);
  private generation = 0;

  /** Whether the server reported another page for the current query. */
  get hasMore(): boolean {
    return this.nextBatch !== null;
  }

  /** Starts a fresh search, discarding results from any earlier query. */
  search(account: AccountId, query: string): Promise<void> {
    const gen = ++this.generation;
    this.query = query;
    this.rooms = [];
    this.totalEstimate = null;
    this.nextBatch = null;
    return this.fetch(account, null, gen);
  }

  /** Appends the next page for the current query. No-op while loading or when no page remains. */
  loadMore(account: AccountId): Promise<void> {
    if (this.loading || this.nextBatch === null) return Promise.resolve();
    return this.fetch(account, this.nextBatch, this.generation);
  }

  private async fetch(account: AccountId, since: string | null, gen: number): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const res = await client.invoke({
        action: 'SearchPublicRooms',
        params: {
          account_id: account,
          query: this.query.trim() || null,
          server: null,
          since,
          limit: DIRECTORY_PAGE_SIZE,
        },
      });
      if (gen !== this.generation) return;
      const page = expect(res, 'PublicRooms').payload;
      this.rooms = [...this.rooms, ...page.rooms];
      this.totalEstimate = page.total_estimate;
      this.nextBatch = page.next_batch;
    } catch (err) {
      if (gen === this.generation) this.error = errorMessage(err);
    } finally {
      if (gen === this.generation) this.loading = false;
    }
  }
}

export const directoryStore = new DirectoryStore();
