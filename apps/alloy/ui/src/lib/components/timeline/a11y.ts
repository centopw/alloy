/**
 * Conversation accessibility model: which rows the keyboard cursor can land on, which arrivals a
 * screen reader should hear, how a chatty burst is coalesced into one announcement, and when a
 * programmatic scroll may animate. Pure logic, so it is unit-testable without a DOM.
 *
 * The model (see docs/audits/a11y-2026-10.md A11Y-02/04/05/22):
 * - the virtualized scroll container is a named `feed` and the conversation's single tab stop;
 *   the arrow keys move DOM focus between message rows (`article`, `tabindex="-1"`), so a long
 *   conversation adds no tab stops;
 * - incoming messages are announced through one dedicated polite live region, never by making the
 *   (virtualized) rows live;
 * - a burst settles into a single announcement after `ANNOUNCE_SETTLE_MS`, but never later than
 *   `ANNOUNCE_MAX_HOLD_MS` after its first message, so a chatty room cannot defer it forever;
 * - JS-driven smooth scrolling is dropped when either the OS or the app asks for reduced motion.
 */
import type { TimelineItemDto } from '$lib/generated';

/** Rows the message cursor can focus: everything that carries a message body. */
const CURSOR_KINDS: Record<string, true> = {
  TextMessage: true,
  Media: true,
  Location: true,
  Deleted: true,
};

/** Arrivals worth announcing. System lines and dividers are not messages. */
const ANNOUNCE_KINDS: Record<string, true> = {
  TextMessage: true,
  Media: true,
  Location: true,
};

/** Whether the roving message cursor may focus this row. */
export function isCursorRow(item: TimelineItemDto): boolean {
  return CURSOR_KINDS[item.content.kind] === true;
}

/** Whether this arrival should produce (or join) a screen-reader announcement. Own echoes do not. */
export function isAnnounceableArrival(item: TimelineItemDto): boolean {
  return !item.is_own && ANNOUNCE_KINDS[item.content.kind] === true;
}

/** A move of the message cursor within the focused conversation. */
export type CursorMove = 'previous' | 'next' | 'page-up' | 'page-down' | 'first' | 'last';

/** What a key does while the conversation or one of its message rows has focus. */
export type TimelineKeyAction = CursorMove | 'open-actions' | 'dismiss-actions';

export interface KeyModifiers {
  shift?: boolean;
  alt?: boolean;
  ctrl?: boolean;
  meta?: boolean;
}

/**
 * Maps a keyboard event to an action. Returns null for anything the conversation must not take
 * over — in particular every chord with Ctrl/Meta/Alt, so browser and OS shortcuts keep working.
 */
export function timelineActionForKey(key: string, modifiers: KeyModifiers = {}): TimelineKeyAction | null {
  const { shift = false, alt = false, ctrl = false, meta = false } = modifiers;
  if (ctrl || meta || alt) return null;
  // Shift+F10 is the standard "open context menu for the focused item" chord.
  if (shift) return key === 'F10' ? 'open-actions' : null;
  switch (key) {
    case 'ArrowUp':
      return 'previous';
    case 'ArrowDown':
      return 'next';
    case 'PageUp':
      return 'page-up';
    case 'PageDown':
      return 'page-down';
    case 'Home':
      return 'first';
    case 'End':
      return 'last';
    case 'Enter':
    case ' ':
      return 'open-actions';
    case 'Escape':
      return 'dismiss-actions';
    default:
      return null;
  }
}

/** How many rows one PageUp/PageDown moves the message cursor while a row is focused. */
export const CURSOR_PAGE_ROWS = 8;

/**
 * Position after moving `action` from `position` in a list of `count` focusable rows. The result is
 * always a valid position, so holding a key at either end never throws focus out of the list.
 */
export function moveCursorPosition(
  position: number,
  count: number,
  action: CursorMove,
  pageRows = CURSOR_PAGE_ROWS,
): number {
  if (count <= 0) return 0;
  const last = count - 1;
  const clamp = (value: number) => Math.max(0, Math.min(value, last));
  switch (action) {
    case 'previous':
      return clamp(position - 1);
    case 'next':
      return clamp(position + 1);
    case 'page-up':
      return clamp(position - Math.max(1, pageRows));
    case 'page-down':
      return clamp(position + Math.max(1, pageRows));
    case 'first':
      return 0;
    case 'last':
      return last;
  }
}

/** Longest preview in an announcement or a row label, in Unicode code points. */
export const PREVIEW_MAX_CHARS = 80;

/** Collapses whitespace and shortens `text` to one short, speakable line. */
export function shortenPreview(text: string, max = PREVIEW_MAX_CHARS): string {
  const collapsed = text.replace(/\s+/g, ' ').trim();
  const chars = Array.from(collapsed);
  if (chars.length <= max) return collapsed;
  // Reserve one code point for the ellipsis and never split a surrogate pair.
  return `${chars.slice(0, Math.max(0, max - 1)).join('').trimEnd()}…`;
}

/** Label for a non-text message body, supplied by the UI so the copy is localized. */
export type PreviewLabel = (item: TimelineItemDto) => string;

/**
 * What a screen reader should hear for one message when it is not plain text: the localized kind
 * label ("Photo", "Poll", ...). Text messages use their collapsed, shortened body.
 */
export function messagePreview(item: TimelineItemDto, kindLabel: PreviewLabel, max = PREVIEW_MAX_CHARS): string {
  if (item.content.kind === 'TextMessage') {
    const body = item.content.data.body;
    return body.trim() === '' ? kindLabel(item) : shortenPreview(body, max);
  }
  return kindLabel(item);
}

export interface RowDescriptionLabels {
  senderName: (item: TimelineItemDto) => string;
  time: (item: TimelineItemDto) => string;
  preview: (item: TimelineItemDto) => string;
  /** Localized sentence template for sender, time and preview. */
  description: (sender: string, time: string, preview: string) => string;
}

/** Accessible name of a message row the cursor focuses, e.g. "Mai, 3:42 PM: hello". */
export function describeRow(item: TimelineItemDto, labels: RowDescriptionLabels): string {
  return labels.description(labels.senderName(item), labels.time(item), labels.preview(item));
}

export interface AnnouncementLabels {
  senderName: (item: TimelineItemDto) => string;
  preview: (item: TimelineItemDto) => string;
  /** Localized template for one arriving message. */
  single: (sender: string, preview: string) => string;
  /** Localized template for a coalesced burst; `sender`/`preview` describe the newest message. */
  many: (count: number, sender: string, preview: string) => string;
}

/**
 * The single line an arrival batch produces: the newest message's sender and preview, with the
 * batch size when the burst was coalesced. Null when nothing in the batch is announceable.
 */
export function formatAnnouncement(
  items: readonly TimelineItemDto[],
  labels: AnnouncementLabels,
): string | null {
  const announceable = items.filter(isAnnounceableArrival);
  if (announceable.length === 0) return null;
  const newest = announceable[announceable.length - 1];
  const sender = labels.senderName(newest);
  const preview = labels.preview(newest);
  return announceable.length === 1
    ? labels.single(sender, preview)
    : labels.many(announceable.length, sender, preview);
}

/** Quiet time after the last arrival before its batch is announced. */
export const ANNOUNCE_SETTLE_MS = 900;
/** A batch is announced after this long even if messages keep arriving. */
export const ANNOUNCE_MAX_HOLD_MS = 2000;

export interface AnnouncementFlushOptions {
  /** Suppresses the announcement (the user turned announcements off); the batch is still cleared. */
  muted: boolean;
  labels: AnnouncementLabels;
}

/**
 * Coalesces incoming messages into at most one pending announcement. The caller feeds every
 * arrival with `push`, schedules a flush for the returned delay, and announces `flush()` once.
 * Bursts therefore cost one announcement; a steady stream of messages is still flushed at most
 * `maxHoldMs` after its first arrival.
 */
export class AnnouncementBatcher {
  #pending: TimelineItemDto[] = [];
  #firstAt = 0;
  #lastAt = 0;

  constructor(
    readonly settleMs: number = ANNOUNCE_SETTLE_MS,
    readonly maxHoldMs: number = ANNOUNCE_MAX_HOLD_MS,
  ) {}

  /** Messages collected but not announced yet. */
  get pending(): number {
    return this.#pending.length;
  }

  /**
   * Adds arrivals and returns the delay in milliseconds until the batch should be flushed, or null
   * when there is nothing to announce (all own echoes / non-message rows).
   */
  push(items: readonly TimelineItemDto[], now = Date.now()): number | null {
    const before = this.#pending.length;
    for (const item of items) {
      if (isAnnounceableArrival(item)) this.#pending.push(item);
    }
    if (this.#pending.length === before) return this.#pending.length === 0 ? null : this.delayUntil(now);
    if (before === 0) this.#firstAt = now;
    this.#lastAt = now;
    return this.delayUntil(now);
  }

  /** Milliseconds until the current batch is due, or null while nothing is pending. */
  delayUntil(now = Date.now()): number | null {
    if (this.#pending.length === 0) return null;
    const settledAt = this.#lastAt + Math.max(0, this.settleMs);
    const heldAt = this.#firstAt + Math.max(this.settleMs, this.maxHoldMs);
    return Math.max(0, Math.min(settledAt, heldAt) - now);
  }

  /** Returns the coalesced announcement and clears the batch; muted batches are dropped silently. */
  flush(options: AnnouncementFlushOptions): string | null {
    const batch = this.#pending;
    this.reset();
    if (options.muted) return null;
    return formatAnnouncement(batch, options.labels);
  }

  /** Drops any pending batch without announcing it. */
  reset(): void {
    this.#pending = [];
    this.#firstAt = 0;
    this.#lastAt = 0;
  }
}

export interface ReducedMotionSources {
  /** The OS setting, `matchMedia('(prefers-reduced-motion: reduce)').matches`. */
  system: boolean;
  /** The app's own `data-reduced-motion` flag, set by the theme bootstrap. */
  app: boolean;
}

/** Linear scrolling (smooth animation) is allowed only when neither source asks for reduced motion. */
export function smoothScrollAllowed(sources: ReducedMotionSources): boolean {
  return !sources.system && !sources.app;
}

interface MediaQueryLike {
  matches: boolean;
}

interface MatchMediaWindowLike {
  matchMedia?: (query: string) => MediaQueryLike;
}

interface ReducedMotionRootLike {
  getAttribute?: (name: string) => string | null;
}

/**
 * Reads both reduced-motion sources. Kept separate from `smoothScrollAllowed` so the DOM lookup
 * (which can be absent in tests and SSR) is not part of the pure decision.
 */
export function readReducedMotionSources(
  win: MatchMediaWindowLike | undefined,
  root: ReducedMotionRootLike | undefined,
): ReducedMotionSources {
  let system = false;
  try {
    system = win?.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches === true;
  } catch {
    // A locked-down webview can throw from matchMedia; treat it as "no preference expressed".
    system = false;
  }
  let app = false;
  try {
    app = root?.getAttribute?.('data-reduced-motion') === 'true';
  } catch {
    app = false;
  }
  return { system, app };
}
