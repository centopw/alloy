import { getVersion } from '@tauri-apps/api/app';
import { isTauriShell } from './notify';

/** Shown where there is no shell to ask (the web build, tests, the bridge). */
export const WEB_VERSION = 'web';

/**
 * The installed app version. The shell is the source: Tauri reports the version of `apps/alloy/src-tauri/Cargo.toml`,
 * the only place the version is written.
 */
export async function appVersion(): Promise<string> {
  return isTauriShell() ? getVersion() : WEB_VERSION;
}
