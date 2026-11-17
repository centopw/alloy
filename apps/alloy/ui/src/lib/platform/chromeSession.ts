import { invoke } from '@tauri-apps/api/core';
import { isTauriShell } from './notify';

/** What the shell returns after reading Chrome's cookie store. Cookie values are never logged. */
export interface ChromeSession {
  /** The Messenger cookies Alloy's cookie login asks for, name → value. */
  cookies: Record<string, string>;
  /** The Chrome profile folder they came from ("Default", "Profile 1"). */
  profile: string;
  /** Whether Chrome still holds the session live. */
  active: boolean;
}

/**
 * Chrome's cookie store only exists inside the desktop app on a Mac: the browser build and the
 * phone builds have no Chrome profile, and a Windows or Linux Chrome encrypts differently.
 */
export function isChromeSessionSupported(): boolean {
  if (!isTauriShell()) return false;
  const agent = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  return /Macintosh|Mac OS X/.test(agent);
}

/**
 * Reads the Messenger session out of Chrome now.
 *
 * The only call in the app that opens Chrome's files or the Keychain, so it happens when the
 * person presses the button and at no other time (ADR 0022).
 */
export async function importChromeSession(): Promise<ChromeSession> {
  return invoke<ChromeSession>('import_chrome_session');
}
