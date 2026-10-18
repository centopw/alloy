import { checkForUpdate, installUpdate, type UpdateInfo } from '../platform/updates';
import { openExternal } from '../platform/links';
import { errorMessage } from './errors';

/** Delay before the startup check, so it does not compete with the first sync. */
export const AUTO_CHECK_DELAY_MS = 8000;

class UpdatesStore {
  /** Latest probe result. Null until a check succeeds, and outside Tauri. */
  info = $state<UpdateInfo | null>(null);
  checking = $state(false);
  installing = $state(false);
  error = $state<string | null>(null);
  /** Version the user dismissed. Kept in memory only, so it reappears on the next launch. */
  dismissedVersion = $state<string | null>(null);

  /**
   * Whether the startup check has fired this app run. Plain (non-reactive) on purpose: the banner
   * reads it inside an $effect and must not re-run when it changes.
   */
  autoCheckDone = false;

  /** The available update, unless the user dismissed this version. */
  get pending(): UpdateInfo | null {
    const info = this.info;
    if (!info?.available || info.version === this.dismissedVersion) return null;
    return info;
  }

  /** Probe for a newer release. A silent check swallows failures; otherwise they surface in `error`. */
  async check({ silent = false }: { silent?: boolean } = {}) {
    if (this.checking) return;
    this.checking = true;
    if (!silent) this.error = null;
    try {
      this.info = await checkForUpdate();
    } catch (e) {
      if (!silent) this.error = errorMessage(e);
    } finally {
      this.checking = false;
    }
  }

  /**
   * Desktop: download and install in place (the app restarts on success). Where the app cannot
   * install (`can_install` false, Android), open the release page instead.
   */
  async install() {
    const info = this.info;
    if (!info?.available || this.installing) return;
    this.error = null;
    try {
      if (!info.can_install) {
        await openExternal(info.url);
        return;
      }
      this.installing = true;
      await installUpdate();
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.installing = false;
    }
  }

  /** Hide the banner for the currently offered version. */
  dismiss() {
    if (!this.info) return;
    this.dismissedVersion = this.info.version;
    this.error = null;
  }
}

export const updatesStore = new UpdatesStore();
