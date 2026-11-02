/**
 * Window/task-switcher title. The app.html title is a static fallback for the first paint; the
 * signed-in shell keeps this in sync so a screen reader's window switcher and the OS task
 * switcher name the open room instead of showing "Alloy" for every window.
 */

export const APP_NAME = 'Alloy';

export interface WindowTitleInput {
  /** Name of the open room, or null when the room list is showing. */
  roomName?: string | null;
  /** Unread message count for the account. Only shown while the window is in the background. */
  unreadCount?: number;
  /** Whether the window is focused. A focused window already shows its unread badges. */
  focused?: boolean;
  /** Application name; defaults to "Alloy". */
  appName?: string;
}

/** The unread prefix the title carries: `(3)` for three queued messages, empty otherwise. */
export function unreadPrefix(
  unreadCount: number | undefined,
  focused: boolean | undefined = true
): string {
  if (focused !== false) return '';
  const raw = typeof unreadCount === 'number' && Number.isFinite(unreadCount) ? unreadCount : 0;
  const count = Math.max(0, Math.floor(raw));
  return count > 0 ? `(${count}) ` : '';
}

/** Builds `Room - Alloy`, or `(3) Room - Alloy` with unread messages while backgrounded. */
export function buildWindowTitle(input: WindowTitleInput = {}): string {
  const room = input.roomName?.trim() ?? '';
  const app = input.appName?.trim() || APP_NAME;
  const base = room === '' ? app : `${room} - ${app}`;
  return `${unreadPrefix(input.unreadCount, input.focused)}${base}`;
}

/** Applies the title through a document-like target; returns the title that was set. */
export function applyWindowTitle(
  input: WindowTitleInput,
  target: { title: string } = document
): string {
  const title = buildWindowTitle(input);
  target.title = title;
  return title;
}
