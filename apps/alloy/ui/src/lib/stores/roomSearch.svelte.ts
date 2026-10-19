import { client } from '../ipc/index.js';
import type { EngineCommand, TimelineItemDto } from '../generated/index.js';
import { quote } from '../components/timeline/ReplyQuote.svelte';
import type { EngineTimer } from './prefs.svelte.js';
import { matchesQuery } from '../content/searchHighlight.js';
import { errorMessage } from './errors.js';
import { timelineStore } from './timeline.svelte.js';
import type { EngineResponse, RoomSearchHit } from '../generated/index.js';

type RoomSearchResults = Extract<EngineResponse, { type: 'RoomSearchResults' }>['payload'];
type SearchRoomMessagesParams = Extract<EngineCommand, { action: 'SearchRoomMessages' }>['params'];

const DEBOUNCE_MS = 250;
const PAGE_SIZE = 50;
/** Results shown at most; the panel says when more matched. */
const RESULT_CAP = 200;
/** Older history pages fetched per "Search older messages" press in local mode. */
const OLDER_PAGES = 5;

export interface SearchResult {
  event_id: string;
  sender: string;
  sender_display_name: string | null;
  timestamp_millis: number;
  body: string;
}

/**
 * 'idle': no criteria. 'server': the homeserver answered the text query. 'local': the loaded
 * timeline is searched, because the query has no text or the server cannot search this room.
 */
export type RoomSearchMode = 'idle' | 'server' | 'local';

function fromHit(hit: RoomSearchHit): SearchResult {
  return {
    event_id: hit.event_id,
    sender: hit.sender,
    sender_display_name: hit.sender_display_name,
    timestamp_millis: hit.timestamp_millis,
    body: hit.body,
  };
}

/** Matches over the loaded timeline, newest first. Only messages with an event ID can be jumped to. */
function scanTimeline(items: TimelineItemDto[], query: string, sender: string | null): SearchResult[] {
  const out: SearchResult[] = [];
  for (const item of items) {
    if (!item.event_id) continue;
    if (sender && item.sender !== sender) continue;
    const body = quote(item);
    if (!body) continue;
    if (query && !matchesQuery(body, query)) continue;
    out.push({
      event_id: item.event_id,
      sender: item.sender,
      sender_display_name: item.sender_display_name ?? null,
      timestamp_millis: item.timestamp_millis,
      body,
    });
  }
  return out.reverse();
}

/**
 * State for the in-room message search panel. Text queries go to the homeserver after a 250 ms
 * pause; a sender-only filter, or a room the server cannot search, is answered from the loaded
 * timeline. Each change invalidates in-flight requests, so a stale answer is never shown.
 */
class RoomSearchStore {
  roomId = $state<string | null>(null);
  query = $state('');
  sender = $state<string | null>(null);
  mode = $state<RoomSearchMode>('idle');
  loading = $state(false);
  error = $state<string | null>(null);
  private serverHits = $state<SearchResult[]>([]);
  private nextBatch = $state<string | null>(null);
  private timer: EngineTimer | undefined = undefined;
  private seq = 0;

  private localHits = $derived.by(() => {
    if (!this.roomId || timelineStore.roomId !== this.roomId) return [];
    return scanTimeline(timelineStore.items, this.query.trim(), this.sender);
  });

  /** Everything matched, before the display cap. */
  private all = $derived(
    this.mode === 'server' ? this.serverHits : this.mode === 'local' ? this.localHits : []
  );
  results = $derived(this.all.slice(0, RESULT_CAP));
  capped = $derived(this.all.length > RESULT_CAP);
  /** The homeserver has more matches to fetch. */
  canLoadMore = $derived(this.mode === 'server' && this.nextBatch !== null && !this.loading);
  /** Local results can grow by fetching older history. */
  canSearchOlder = $derived(
    this.mode === 'local' && this.roomId !== null && timelineStore.roomId === this.roomId && !timelineStore.reachedStart
  );

  /** Binds the store to a room. Switching rooms clears the query and results. */
  bind(roomId: string) {
    if (this.roomId === roomId) return;
    this.stop();
    this.roomId = roomId;
    this.query = '';
    this.sender = null;
    this.mode = 'idle';
    this.error = null;
    this.serverHits = [];
    this.nextBatch = null;
  }

  /** Cancels the pending search and invalidates in-flight requests. */
  stop() {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.seq++;
    this.loading = false;
  }

  setQuery(text: string) {
    this.query = text;
    this.schedule();
  }

  setSender(userId: string | null) {
    this.sender = userId;
    this.schedule();
  }

  private schedule() {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.seq++;
    this.error = null;
    this.serverHits = [];
    this.nextBatch = null;
    this.loading = false;

    const text = this.query.trim();
    if (!text && !this.sender) {
      this.mode = 'idle';
      return;
    }
    if (!text) {
      // A sender filter alone is answered from the loaded timeline; an empty query is never sent.
      this.mode = 'local';
      return;
    }
    this.mode = 'server';
    this.loading = true;
    this.timer = setTimeout(() => void this.runServer(), DEBOUNCE_MS);
  }

  private async runServer() {
    this.timer = undefined;
    const roomId = this.roomId;
    const text = this.query.trim();
    if (!roomId || !text) return;
    const seq = this.seq;
    this.loading = true;
    try {
      const page = await this.request({
        room_id: roomId,
        query: text,
        sender: this.sender,
        limit: PAGE_SIZE,
        next_batch: null,
      });
      if (seq !== this.seq) return;
      this.applyPage(page, []);
    } catch (e) {
      if (seq !== this.seq) return;
      this.error = errorMessage(e);
    } finally {
      if (seq === this.seq) this.loading = false;
    }
  }

  /** Fetches the next server page of the current text query. */
  async loadMore() {
    const roomId = this.roomId;
    const text = this.query.trim();
    if (!roomId || !text || !this.canLoadMore || !this.nextBatch) return;
    const seq = this.seq;
    this.loading = true;
    try {
      const page = await this.request({
        room_id: roomId,
        query: text,
        sender: this.sender,
        limit: PAGE_SIZE,
        next_batch: this.nextBatch,
      });
      if (seq !== this.seq) return;
      this.applyPage(page, this.serverHits);
    } catch (e) {
      if (seq !== this.seq) return;
      this.error = errorMessage(e);
    } finally {
      if (seq === this.seq) this.loading = false;
    }
  }

  /** Loads older timeline history, then the local results include whatever it brought in. */
  async searchOlder() {
    if (!this.canSearchOlder) return;
    this.loading = true;
    try {
      for (let i = 0; i < OLDER_PAGES && !timelineStore.reachedStart; i++) {
        const before = timelineStore.items.length;
        await timelineStore.loadOlder(PAGE_SIZE);
        if (timelineStore.error || timelineStore.items.length === before) break;
      }
    } finally {
      this.loading = false;
    }
  }

  /** Applies a server page. Unavailable switches to the local strategy. */
  private applyPage(page: RoomSearchResults, existing: SearchResult[]) {
    if (page.scope === 'Unavailable') {
      this.mode = 'local';
      this.serverHits = [];
      this.nextBatch = null;
      return;
    }
    const seen = new Set(existing.map((h) => h.event_id));
    const fresh = page.hits.filter((h) => !seen.has(h.event_id)).map(fromHit);
    this.mode = 'server';
    this.serverHits = [...existing, ...fresh];
    this.nextBatch = page.next_batch;
  }

  private async request(params: SearchRoomMessagesParams) {
    const res = await client.invoke({ action: 'SearchRoomMessages', params });
    if (res.type !== 'RoomSearchResults') throw new Error(`Expected RoomSearchResults, got ${res.type}`);
    return res.payload;
  }
}

export const roomSearch = new RoomSearchStore();
