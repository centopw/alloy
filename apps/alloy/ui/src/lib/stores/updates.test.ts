import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UpdateInfo, UpdateProgress } from '../platform/updates';
import {
  AUTO_CHECK_DELAY_MS,
  SKIPPED_VERSION_KEY,
  UPDATE_CHECK_INTERVAL_MS,
  UpdatesStore,
  type UpdatesDeps,
} from './updates.svelte';

vi.mock('./prefs.svelte', () => ({ prefs: {} }));
vi.mock('../platform/updates', () => ({
  checkForUpdate: vi.fn(),
  downloadUpdate: vi.fn(),
  applyUpdate: vi.fn(),
  onUpdateDownloaded: vi.fn(() => () => {}),
}));
vi.mock('../platform/links', () => ({ openExternal: vi.fn() }));

const RELEASE_URL = 'https://example.com/releases/1.2.0';

function info(overrides: Partial<UpdateInfo> = {}): UpdateInfo {
  return {
    available: true,
    version: '1.2.0',
    notes: '## Fixes',
    can_install: true,
    url: RELEASE_URL,
    downloaded: false,
    ...overrides,
  };
}

interface Harness {
  deps: UpdatesDeps;
  values: { autoCheckUpdates: boolean; autoDownloadUpdates: boolean };
  storage: Map<string, string>;
}

function harness(overrides: Partial<UpdatesDeps> = {}): Harness {
  const values = { autoCheckUpdates: false, autoDownloadUpdates: false };
  const storage = new Map<string, string>();
  const prefs = {
    get autoCheckUpdates() {
      return values.autoCheckUpdates;
    },
    get autoDownloadUpdates() {
      return values.autoDownloadUpdates;
    },
    set: (key: keyof typeof values, value: boolean) => {
      values[key] = value;
    },
  };
  const deps: UpdatesDeps = {
    check: vi.fn(async () => info({ available: false, version: '1.0.0' })),
    download: vi.fn(async (_onProgress: (p: UpdateProgress) => void) => {}),
    apply: vi.fn(async () => {}),
    onDownloaded: vi.fn(() => () => {}),
    open: vi.fn(async (_url: string) => {}),
    prefs: prefs as UpdatesDeps['prefs'],
    storage: {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
    },
    now: () => 1000,
    ...overrides,
  };
  return { deps, values, storage };
}

describe('UpdatesStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('offers a manual update and records the check time when auto-download is off', async () => {
    const { deps } = harness({ check: vi.fn(async () => info()) });
    const store = new UpdatesStore(deps);

    await store.check();

    expect(store.phase).toBe('available');
    expect(store.primaryLabel).toBe('Download update');
    expect(store.lastCheckedAt).toBe(1000);
    expect(deps.download).not.toHaveBeenCalled();
  });

  it('downloads automatically and reports ready when auto-download is on', async () => {
    const { deps, values } = harness({ check: vi.fn(async () => info()) });
    values.autoDownloadUpdates = true;
    const store = new UpdatesStore(deps);

    await store.check();
    await vi.waitFor(() => expect(store.phase).toBe('ready'));

    expect(deps.download).toHaveBeenCalledTimes(1);
    expect(store.info?.downloaded).toBe(true);
    expect(store.primaryLabel).toBe('Restart to apply update');
  });

  it('reports byte progress and falls back when the total is unknown', async () => {
    let finish: () => void = () => {};
    const download = vi.fn((onProgress: (p: UpdateProgress) => void) => {
      onProgress({ downloaded: 50, total: 100 });
      return new Promise<void>((resolve) => {
        finish = resolve;
      });
    });
    const { deps } = harness({ download });
    const store = new UpdatesStore(deps);
    store.info = info();

    const running = store.download();
    expect(store.phase).toBe('downloading');
    expect(store.primaryLabel).toBe('Downloading 50%');

    store.progress = null;
    expect(store.primaryLabel).toBe('Downloading update...');
    expect(store.busy).toBe(true);

    finish();
    await running;
    expect(store.phase).toBe('ready');
    expect(store.progress).toBeNull();
  });

  it('keeps a download failure visible and does not retry it automatically', async () => {
    const { deps, values } = harness({
      check: vi.fn(async () => info()),
      download: vi.fn(async () => {
        throw new Error('network down');
      }),
    });
    values.autoDownloadUpdates = true;
    const store = new UpdatesStore(deps);

    await store.check();
    await vi.waitFor(() => expect(store.phase).toBe('error'));
    expect(store.error).toContain('network down');

    await store.check();
    expect(deps.download).toHaveBeenCalledTimes(1);
  });

  it('restarts when downloaded and downloads otherwise from primary()', async () => {
    const { deps } = harness();
    const store = new UpdatesStore(deps);

    store.info = info({ downloaded: true });
    await store.primary();
    expect(deps.apply).toHaveBeenCalledTimes(1);

    const fresh = new UpdatesStore(deps);
    fresh.info = info();
    await fresh.primary();
    expect(deps.download).toHaveBeenCalledTimes(1);
  });

  it('opens the release page instead of installing where in-place install is unavailable', async () => {
    const { deps } = harness();
    const store = new UpdatesStore(deps);
    store.info = info({ can_install: false });

    expect(store.primaryLabel).toBe('Open release page');
    await store.primary();

    expect(deps.open).toHaveBeenCalledWith(RELEASE_URL);
    expect(deps.download).not.toHaveBeenCalled();
    expect(deps.apply).not.toHaveBeenCalled();
  });

  it('surfaces install failures', async () => {
    const { deps } = harness({
      apply: vi.fn(async () => {
        throw new Error('boom');
      }),
    });
    const store = new UpdatesStore(deps);
    store.info = info({ downloaded: true });

    await store.apply();

    expect(store.phase).toBe('error');
    expect(store.error).toContain('boom');
  });

  it('persists a skipped version and hides it on the next launch', async () => {
    const { deps, storage } = harness({ check: vi.fn(async () => info()) });
    const store = new UpdatesStore(deps);
    await store.check();

    store.skipVersion();

    expect(store.pending).toBeNull();
    expect(storage.get(SKIPPED_VERSION_KEY)).toBe('1.2.0');

    const next = new UpdatesStore({ ...deps, check: vi.fn(async () => info()) });
    expect(next.skippedVersion).toBe('1.2.0');
    await next.check();
    expect(next.pending).toBeNull();
  });

  it('hides the offer for this run after remind-me-later', async () => {
    const { deps } = harness({ check: vi.fn(async () => info()) });
    const store = new UpdatesStore(deps);
    await store.check();

    store.remindLater();

    expect(store.pending).toBeNull();
    expect(store.info?.available).toBe(true);
  });

  it('schedules background checks and stops them when auto-check is turned off', async () => {
    const { deps, values } = harness();
    values.autoCheckUpdates = true;
    const store = new UpdatesStore(deps);
    const teardown = store.start();

    expect(deps.check).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(AUTO_CHECK_DELAY_MS);
    expect(deps.check).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(UPDATE_CHECK_INTERVAL_MS);
    expect(deps.check).toHaveBeenCalledTimes(2);

    store.setAutoCheck(false);
    expect(values.autoCheckUpdates).toBe(false);
    await vi.advanceTimersByTimeAsync(UPDATE_CHECK_INTERVAL_MS * 2);
    expect(deps.check).toHaveBeenCalledTimes(2);

    teardown();
  });

  it('does not surface silent background check failures', async () => {
    const { deps } = harness({
      check: vi.fn(async () => {
        throw new Error('offline');
      }),
    });
    const store = new UpdatesStore(deps);

    await store.check({ silent: true });

    expect(store.error).toBeNull();
  });

  it('shows a manual check failure', async () => {
    const { deps } = harness({
      check: vi.fn(async () => {
        throw new Error('offline');
      }),
    });
    const store = new UpdatesStore(deps);

    await store.check();

    expect(store.error).toContain('offline');
  });
});
