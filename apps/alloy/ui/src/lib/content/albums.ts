import type { TimelineItemDto } from '$lib/generated';
import { isFileName } from './format';

/** Pictures and videos sent one after another within this gap belong to one album. */
export const ALBUM_GAP_MS = 60_000;
/** The most pictures one album shows; a longer run starts a second album. */
export const ALBUM_MAX = 10;

/** Which rows an album swallows: its first row renders the grid, the others render nothing. */
export interface AlbumLayout {
  /** First row id -> every item of its album, in timeline order. */
  leads: ReadonlyMap<string, readonly TimelineItemDto[]>;
  /** Rows folded into the album of an earlier row. */
  hidden: ReadonlySet<string>;
}

/** A captionless picture or video that can sit in a grid: no reply, reactions or thread to lose. */
export function isAlbumCandidate(item: TimelineItemDto): boolean {
  const c = item.content;
  if (c.kind !== 'Media' || !item.event_id) return false;
  if (c.data.media_kind !== 'image' && c.data.media_kind !== 'video') return false;
  if (item.reply_to || item.thread_summary || (item.reactions ?? []).length > 0) return false;
  if (item.send_state.status !== 'Sent') return false;
  return !c.data.body || isFileName(c.data.body);
}

/** Groups consecutive candidates from one sender into albums of two or more. */
export function groupAlbums(rows: readonly TimelineItemDto[]): AlbumLayout {
  const leads = new Map<string, TimelineItemDto[]>();
  const hidden = new Set<string>();
  let run: TimelineItemDto[] = [];

  const flush = () => {
    if (run.length >= 2) {
      leads.set(run[0].internal_id, run);
      for (const item of run.slice(1)) hidden.add(item.internal_id);
    }
    run = [];
  };

  for (const row of rows) {
    if (!isAlbumCandidate(row)) {
      flush();
      continue;
    }
    const last = run[run.length - 1];
    const joins =
      last !== undefined &&
      last.sender === row.sender &&
      row.timestamp_millis - last.timestamp_millis <= ALBUM_GAP_MS &&
      run.length < ALBUM_MAX;
    if (!joins) flush();
    run.push(row);
  }
  flush();
  return { leads, hidden };
}
