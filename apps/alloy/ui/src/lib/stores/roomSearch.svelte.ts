import { client } from '../ipc/index.js';
import type { TimelineItemDto } from '../generated/index.js';
import { quote } from '../components/timeline/ReplyQuote.svelte';
import type { RoomSearchHit } from '../generated/index.js';
import { matchesQuery } from '../content/searchHighlight.js';
import { errorMessage, expect } from './errors.js';
import { timelineStore } from './timeline.svelte.js';

const DEBOUNCE_MS = 250;
/** Messages asked of the store's full-text index per search. */
export const ROOM_SEARCH_LIMIT = 100;
/** Results shown at most; the panel says when more matched. */
const RESULT_CAP = 200;

export interface SearchResult {
  event_id: string;
  sender: string;
  sender_display_name: string | null;
  timestamp_millis: number;
  body: string;
}

/**
 * 'idle': no criteria. 'search': the local full-text index answered the text query. 'local': only a
 * sender was chosen, so the loaded timeline is filtered (there is no text to index).
 */
export type RoomSearchMode = 'idle' | 'search' | 'local';

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
 * State for the in-room message search panel. Text queries go to the local full-text index after a
 * 250 ms pause; a sender-only filter is answered from the loaded timeline. Each change invalidates
 * in-flight requests, so a stale answer is never shown.
 */
class RoomSearchStore {
  roomId = $state<string | null>(null);
  query = $state('');
  sender = $state<string | null>(null);
  mode = $state<RoomSearchMode>('idle');
  loading = $state(false);
  error = $state<string | null>(null);
  private found = $state<SearchResult[]>([]);
  private timer: ReturnType<typeof setTimeout> | undefined = undefined;
  private seq = 0;

  private localHits = $derived.by(() => {
    if (!this.roomId || this.sender === null || timelineStore.roomId !== this.roomId) return [];
    return scanTimeline(timelineStore.items, this.query.trim(), this.sender);
  });

  /** The text-index hits narrowed by the sender filter. */
  private hits = $derived(
    this.mode === 'search' && this.sender !== null
      ? this.found.filter((hit) => hit.sender === this.sender)
      : this.found
  );

  private all = $derived(this.mode === 'local' ? this.localHits : this.hits);
  results = $derived(this.all.slice(0, RESULT_CAP));
  capped = $derived(this.all.length > RESULT_CAP);

  /** Binds the store to a room. Switching rooms clears the query and results. */
  bind(roomId: string) {
    if (this.roomId === roomId) return;
    this.stop();
    this.roomId = roomId;
    this.query = '';
    this.sender = null;
    this.mode = 'idle';
    this.error = null;
    this.found = [];
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
    this.found = [];
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
    this.mode = 'search';
    this.loading = true;
    this.timer = setTimeout(() => void this.run(), DEBOUNCE_MS);
  }

  private async run() {
    this.timer = undefined;
    const roomId = this.roomId;
    const text = this.query.trim();
    if (!roomId || !text) return;
    const seq = this.seq;
    this.loading = true;
    try {
      const res = await client.invoke({
        action: 'SearchRoomMessages',
        params: { room_id: roomId, query: text, limit: ROOM_SEARCH_LIMIT },
      });
      if (seq !== this.seq) return;
      this.found = expect(res, 'RoomSearchResults').payload.hits.map(fromHit);
      this.error = null;
    } catch (e) {
      if (seq !== this.seq) return;
      this.error = errorMessage(e);
    } finally {
      if (seq === this.seq) this.loading = false;
    }
  }
}

export const roomSearch = new RoomSearchStore();
