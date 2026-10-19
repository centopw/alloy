import {
  applyUpdate,
  checkForUpdate,
  downloadUpdate,
  onUpdateDownloaded,
  type UpdateInfo,
  type UpdateProgress,
} from '../platform/updates';
import { openExternal } from '../platform/links';
import { errorMessage } from './errors';
import { prefs, type PrefsStore } from './prefs.svelte';

/** Delay before the startup check, so it does not compete with the first sync. */
export const AUTO_CHECK_DELAY_MS = 8000;
/** How often a running app checks again for a newer release. */
export const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
/** Storage key holding the version the user chose to skip. */
export const SKIPPED_VERSION_KEY = 'alloy.updates.skipped';

/**
 * Where the update flow stands. A failed download or install sets `error`. A failed check only sets
 * `error` for manual checks and keeps the phase.
 */
export type UpdatePhase = 'idle' | 'available' | 'downloading' | 'ready' | 'installing' | 'error';

/** Side effects of the update flow, injectable so the state machine can be tested without Tauri. */
export interface UpdatesDeps {
  check: () => Promise<UpdateInfo | null>;
  download: (onProgress: (progress: UpdateProgress) => void) => Promise<void>;
  apply: () => Promise<void>;
  onDownloaded: (cb: () => void) => () => void;
  open: (url: string) => Promise<void>;
  prefs: Pick<PrefsStore, 'autoCheckUpdates' | 'autoDownloadUpdates' | 'set'>;
  storage: Pick<Storage, 'getItem' | 'setItem'> | null;
  now: () => number;
}

export class UpdatesStore {
  info = $state<UpdateInfo | null>(null);
  phase = $state<UpdatePhase>('idle');
  /** Download fraction from 0 to 1 while downloading. Null when the size is unknown. */
  progress = $state<number | null>(null);
  checking = $state(false);
  error = $state<string | null>(null);
  lastCheckedAt = $state<number | null>(null);
  /** Version the user chose to skip. Persisted, so it stays hidden across launches. */
  skippedVersion = $state<string | null>(null);
  /** Version the user chose to be reminded about later. Hidden for this run only. */
  laterVersion = $state<string | null>(null);
  changelogOpen = $state(false);

  readonly #deps: UpdatesDeps;
  #timer: number | undefined = undefined;
  #interval: number | undefined = undefined;
  #unlistenDownloaded: (() => void) | undefined = undefined;
  /** Version whose automatic download already started, so a failed one is not retried in a loop. */
  #autoStartedVersion: string | null = null;

  constructor(deps: Partial<UpdatesDeps> = {}) {
    this.#deps = {
      check: checkForUpdate,
      download: downloadUpdate,
      apply: applyUpdate,
      onDownloaded: onUpdateDownloaded,
      open: openExternal,
      prefs,
      storage: typeof localStorage === 'undefined' ? null : localStorage,
      now: Date.now,
      ...deps,
    };
    this.skippedVersion = this.#readSkipped();
  }

  /** The available update the UI should offer, unless the user skipped or deferred it. */
  get pending(): UpdateInfo | null {
    const info = this.info;
    if (!info?.available) return null;
    if (info.version === this.skippedVersion || info.version === this.laterVersion) return null;
    return info;
  }

  /** Label of the primary action: open the release page, download, restart, or the current step. */
  get primaryLabel(): string {
    const info = this.info;
    if (info && !info.can_install) return 'Open release page';
    if (this.phase === 'downloading') {
      return this.progress === null ? 'Downloading update...' : `Downloading ${Math.round(this.progress * 100)}%`;
    }
    if (this.phase === 'installing') return 'Installing update...';
    if (info?.downloaded) return 'Restart to apply update';
    return 'Download update';
  }

  /** Whether a download or install is running; the primary action and settings controls wait on it. */
  get busy(): boolean {
    return this.phase === 'downloading' || this.phase === 'installing';
  }

  /** Begin background checks and listen for finished downloads. Returns the teardown. Call once from the app shell. */
  start(): () => void {
    this.#unlistenDownloaded = this.#deps.onDownloaded(() => this.#markDownloaded());
    this.reconcile();
    return () => {
      this.#stop();
      this.#unlistenDownloaded?.();
      this.#unlistenDownloaded = undefined;
    };
  }

  /** Schedule or cancel background checks to match `prefs.autoCheckUpdates`. */
  reconcile(): void {
    this.#stop();
    if (!this.#deps.prefs.autoCheckUpdates) return;
    this.#timer = window.setTimeout(() => {
      this.#timer = undefined;
      void this.check({ silent: true });
      this.#interval = window.setInterval(() => void this.check({ silent: true }), UPDATE_CHECK_INTERVAL_MS);
    }, AUTO_CHECK_DELAY_MS);
  }

  /** Probe for a newer release. A silent check swallows failures; otherwise they surface in `error`. */
  async check({ silent = false }: { silent?: boolean } = {}): Promise<void> {
    if (this.checking || this.phase === 'downloading' || this.phase === 'installing') return;
    this.checking = true;
    if (!silent) this.error = null;
    try {
      const info = await this.#deps.check();
      this.lastCheckedAt = this.#deps.now();
      this.#applyInfo(info);
      this.#maybeAutoDownload();
    } catch (e) {
      if (!silent) this.error = errorMessage(e);
    } finally {
      this.checking = false;
    }
  }

  /** Download and verify the available update without installing it. No-op where the app cannot install in place. */
  async download(): Promise<void> {
    const info = this.info;
    if (!info?.available || !info.can_install) return;
    if (this.phase === 'downloading' || this.phase === 'installing' || this.phase === 'ready') return;
    this.error = null;
    this.phase = 'downloading';
    this.progress = 0;
    try {
      await this.#deps.download((p) => {
        this.progress = p.total && p.total > 0 ? Math.min(1, p.downloaded / p.total) : null;
      });
      this.#markDownloaded();
    } catch (e) {
      this.phase = 'error';
      this.progress = null;
      this.error = errorMessage(e);
    }
  }

  /** Install the downloaded update, which restarts the app. Where the app cannot install, opens the release page. */
  async apply(): Promise<void> {
    const info = this.info;
    if (!info?.available) return;
    if (!info.can_install) return this.openReleasePage();
    if (this.phase === 'downloading' || this.phase === 'installing') return;
    this.error = null;
    this.phase = 'installing';
    try {
      await this.#deps.apply();
      // Normally the app restarts and this never runs; if it returns, the update is still ready.
      this.phase = 'ready';
    } catch (e) {
      this.phase = 'error';
      this.error = errorMessage(e);
    }
  }

  /** The primary action for the current state: restart when downloaded, otherwise download, or open the release page. */
  async primary(): Promise<void> {
    const info = this.info;
    if (!info?.available) return;
    if (!info.can_install) return this.openReleasePage();
    if (info.downloaded) return this.apply();
    return this.download();
  }

  async openReleasePage(): Promise<void> {
    const url = this.info?.url;
    if (!url) return;
    try {
      await this.#deps.open(url);
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** Stop offering the current version, now and on later launches. */
  skipVersion(): void {
    const version = this.info?.version;
    if (!version) return;
    this.skippedVersion = version;
    try {
      this.#deps.storage?.setItem(SKIPPED_VERSION_KEY, version);
    } catch {
      // Storage is unavailable: the skip still holds for this run.
    }
  }

  /** Hide the offer for the current version until the next launch. */
  remindLater(): void {
    this.laterVersion = this.info?.version ?? null;
  }

  /** Turn periodic checks on or off and persist the choice. */
  setAutoCheck(enabled: boolean): void {
    this.#deps.prefs.set('autoCheckUpdates', enabled);
    this.reconcile();
  }

  /** Turn background download on or off and persist the choice. Enabling it starts the download right away when one is pending. */
  setAutoDownload(enabled: boolean): void {
    this.#deps.prefs.set('autoDownloadUpdates', enabled);
    this.#autoStartedVersion = null;
    this.#maybeAutoDownload();
  }

  openChangelog(): void {
    this.changelogOpen = true;
  }

  closeChangelog(): void {
    this.changelogOpen = false;
  }

  #applyInfo(info: UpdateInfo | null): void {
    this.info = info;
    if (!info?.available) this.phase = 'idle';
    else this.phase = info.downloaded ? 'ready' : 'available';
  }

  #markDownloaded(): void {
    if (!this.info?.available || this.phase === 'installing') return;
    this.info = { ...this.info, downloaded: true };
    this.phase = 'ready';
    this.progress = null;
    this.error = null;
  }

  #maybeAutoDownload(): void {
    const info = this.info;
    if (!info?.available || !info.can_install || info.downloaded) return;
    if (!this.#deps.prefs.autoDownloadUpdates || info.version === this.skippedVersion) return;
    if (this.#autoStartedVersion === info.version) return;
    this.#autoStartedVersion = info.version;
    void this.download();
  }

  #stop(): void {
    window.clearTimeout(this.#timer);
    window.clearInterval(this.#interval);
    this.#timer = undefined;
    this.#interval = undefined;
  }

  #readSkipped(): string | null {
    try {
      return this.#deps.storage?.getItem(SKIPPED_VERSION_KEY) ?? null;
    } catch {
      return null;
    }
  }
}

export const updatesStore = new UpdatesStore();
