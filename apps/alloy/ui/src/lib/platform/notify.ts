import { invoke } from '@tauri-apps/api/core';

/** True inside the Tauri shell (desktop or Android); false in the plain browser build. */
export function isTauriShell(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export interface OsNotification {
  title: string;
  body: string;
  /** When true the notification is shown without a sound. */
  silent: boolean;
}

/**
 * Asks for permission to show OS notifications and resolves whether they may be shown.
 * The browser build falls back to the Web Notification API.
 */
export async function requestNotificationPermission(): Promise<boolean> {
  if (isTauriShell()) return invoke<boolean>('notify_permission');
  if (typeof Notification === 'undefined') return false;
  if (Notification.permission === 'granted') return true;
  if (Notification.permission === 'denied') return false;
  return (await Notification.requestPermission()) === 'granted';
}

/** Shows one OS notification. Rejects when permission is not granted. */
export async function showNotification(n: OsNotification): Promise<void> {
  if (isTauriShell()) {
    await invoke<void>('notify_message', { title: n.title, body: n.body, silent: n.silent });
    return;
  }
  if (typeof Notification === 'undefined' || Notification.permission !== 'granted') {
    throw new Error('notification permission is not granted');
  }
  new Notification(n.title, { body: n.body, silent: n.silent });
}
