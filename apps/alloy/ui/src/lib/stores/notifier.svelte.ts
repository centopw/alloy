import { untrack } from 'svelte';
import type { NotificationLevel, RoomSummary } from '../generated/index.js';
import { showNotification, type OsNotification } from '../platform/notify.js';
import { errorMessage } from './errors.js';
import { prefs } from './prefs.svelte.js';
import { roomNotify } from './roomNotify.svelte.js';
import { roomsStore, roomTitle } from './rooms.svelte.js';

/** At most one OS notification per room within this window; later messages replace the queued one. */
export const COALESCE_MS = 3000;

/** A room as the notifier sees it: plain values, so observing never tracks reactive state. */
export interface RoomSnapshot {
  id: string;
  title: string;
  /** Latest message body, or null when the room has none yet. */
  preview: string | null;
  notifications: number;
  highlights: number;
}

export interface NotifierSettings {
  desktop: boolean;
  sound: boolean;
  preview: boolean;
  onlyWhenUnfocused: boolean;
}

export interface NotifyState {
  /** Effective level the engine resolves for the room. */
  level: NotificationLevel;
  /** New notifications since the last observation of the room. */
  newNotifications: number;
  /** New highlights (mentions and keywords) since the last observation of the room. */
  newHighlights: number;
  /** The room is the one open in the shell. */
  viewingRoom: boolean;
  windowFocused: boolean;
  settings: NotifierSettings;
}

/** Whether a room's new activity should produce an OS notification. Pure so the rules are testable. */
export function shouldNotify(s: NotifyState): boolean {
  if (!s.settings.desktop) return false;
  if (s.level === 'Mute') return false;
  if (s.newNotifications <= 0) return false;
  if (s.level === 'Mentions' && s.newHighlights <= 0) return false;
  if (s.windowFocused && (s.viewingRoom || s.settings.onlyWhenUnfocused)) return false;
  return true;
}

/** Maps room summaries to the snapshot fields the notifier reads. */
export function snapshotRooms(rooms: RoomSummary[]): RoomSnapshot[] {
  return rooms.map((r) => ({
    id: r.id,
    title: roomTitle(r),
    preview: r.latest_event?.body ?? null,
    notifications: r.unread_notifications.notification_count,
    highlights: r.unread_notifications.highlight_count,
  }));
}

/** Side effects the notifier needs. The live store wires them to the engine, prefs and the OS. */
export interface NotifierDeps {
  now(): number;
  windowFocused(): boolean;
  activeRoomId(): string | null;
  levelOf(roomId: string): Promise<NotificationLevel>;
  show(n: OsNotification): Promise<void>;
  settings(): NotifierSettings;
}

interface QueuedNotice {
  notice: OsNotification;
  timer: ReturnType<typeof setTimeout>;
}

/**
 * Turns unread-count changes in the room list into OS notifications.
 *
 * The first sighting of a room only records its counts, so the initial sync does not notify for
 * everything already unread. Later increases are checked against the room's engine level, the
 * notification preferences and the window focus, then coalesced per room.
 */
export class Notifier {
  /** Last observed counts per room. Rooms that leave the list are dropped, so a return re-baselines. */
  private seen = new Map<string, RoomSnapshot>();
  private lastShown = new Map<string, number>();
  private queued = new Map<string, QueuedNotice>();
  private stopWatch: (() => void) | null = null;
  /** Last failure to read a level or show a notification. Null when the last attempt succeeded. */
  error = $state<string | null>(null);

  constructor(private readonly deps: NotifierDeps) {}

  /** Starts watching the room list. Calling it again restarts the watch from a fresh baseline. */
  start(): void {
    this.stop();
    this.stopWatch = $effect.root(() => {
      $effect(() => {
        const rooms = snapshotRooms(roomsStore.rooms);
        untrack(() => this.observe(rooms));
      });
    });
  }

  /** Stops watching and drops queued notifications. */
  stop(): void {
    this.stopWatch?.();
    this.stopWatch = null;
    for (const q of this.queued.values()) clearTimeout(q.timer);
    this.queued.clear();
    this.seen.clear();
  }

  /** Compares the room list with the previous observation and notifies for new activity. */
  observe(rooms: RoomSnapshot[]): void {
    const prior = this.seen;
    this.seen = new Map(rooms.map((room): [string, RoomSnapshot] => [room.id, room]));
    for (const room of rooms) {
      const before = prior.get(room.id);
      if (!before) continue;
      const notifications = room.notifications - before.notifications;
      if (notifications <= 0) continue;
      void this.consider(room, notifications, room.highlights - before.highlights);
    }
  }

  private async consider(room: RoomSnapshot, newNotifications: number, newHighlights: number): Promise<void> {
    if (!this.deps.settings().desktop) return;
    let level: NotificationLevel;
    try {
      level = await this.deps.levelOf(room.id);
      this.error = null;
    } catch (err) {
      this.error = errorMessage(err);
      return;
    }
    const settings = this.deps.settings();
    const state: NotifyState = {
      level,
      newNotifications,
      newHighlights,
      viewingRoom: this.deps.activeRoomId() === room.id,
      windowFocused: this.deps.windowFocused(),
      settings,
    };
    if (!shouldNotify(state)) return;
    const body = settings.preview ? (room.preview ?? 'New message') : 'New message';
    this.deliver(room.id, { title: room.title, body, silent: !settings.sound, roomId: room.id });
  }

  private deliver(roomId: string, notice: OsNotification): void {
    const now = this.deps.now();
    const last = this.lastShown.get(roomId);
    if (last === undefined || now - last >= COALESCE_MS) {
      this.send(roomId, notice);
      return;
    }
    const queued = this.queued.get(roomId);
    if (queued) {
      queued.notice = notice;
      return;
    }
    const timer = setTimeout(() => {
      const next = this.queued.get(roomId);
      this.queued.delete(roomId);
      if (next) this.send(roomId, next.notice);
    }, COALESCE_MS - (now - last));
    this.queued.set(roomId, { notice, timer });
  }

  private send(roomId: string, notice: OsNotification): void {
    this.lastShown.set(roomId, this.deps.now());
    this.deps.show(notice).then(
      () => {
        this.error = null;
      },
      (err: unknown) => {
        this.error = errorMessage(err);
      }
    );
  }
}

export const notifier = new Notifier({
  now: () => Date.now(),
  windowFocused: () => typeof document !== 'undefined' && document.hasFocus(),
  activeRoomId: () => roomsStore.activeRoomId,
  levelOf: async (roomId) => {
    await roomNotify.load(roomId);
    const level = roomNotify.levelOf(roomId);
    if (level === null) throw new Error(roomNotify.error(roomId) ?? 'room notification level unknown');
    return level;
  },
  show: showNotification,
  settings: () => ({
    desktop: prefs.notifyDesktop,
    sound: prefs.notifySound,
    preview: prefs.notifyPreview,
    onlyWhenUnfocused: prefs.notifyOnlyWhenUnfocused,
  }),
});
