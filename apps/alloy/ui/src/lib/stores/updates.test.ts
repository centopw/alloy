import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UpdateInfo } from '../platform/updates';

const mocks = vi.hoisted(() => ({
  checkForUpdate: vi.fn(),
  installUpdate: vi.fn(),
  openExternal: vi.fn(),
}));

vi.mock('../platform/updates', () => ({
  checkForUpdate: mocks.checkForUpdate,
  installUpdate: mocks.installUpdate,
}));

vi.mock('../platform/links', () => ({
  openExternal: mocks.openExternal,
}));

import { updatesStore } from './updates.svelte';

const RELEASE_URL = 'https://example.org/releases/1.2.0';

const AVAILABLE: UpdateInfo = {
  available: true,
  version: '1.2.0',
  notes: null,
  can_install: true,
  url: RELEASE_URL,
};

const UP_TO_DATE: UpdateInfo = { ...AVAILABLE, available: false, version: '1.1.0' };

function resetStore() {
  updatesStore.info = null;
  updatesStore.checking = false;
  updatesStore.installing = false;
  updatesStore.error = null;
  updatesStore.dismissedVersion = null;
}

beforeEach(() => {
  vi.clearAllMocks();
  resetStore();
});

afterEach(() => {
  resetStore();
});

describe('updatesStore.check', () => {
  it('stores the probe result', async () => {
    mocks.checkForUpdate.mockResolvedValue(AVAILABLE);
    await updatesStore.check();
    expect(updatesStore.info).toEqual(AVAILABLE);
    expect(updatesStore.checking).toBe(false);
  });

  it('silent check swallows errors and never sets error', async () => {
    mocks.checkForUpdate.mockRejectedValue(new Error('offline'));
    await expect(updatesStore.check({ silent: true })).resolves.toBeUndefined();
    expect(updatesStore.error).toBeNull();
    expect(updatesStore.info).toBeNull();
    expect(updatesStore.checking).toBe(false);
  });

  it('non-silent check sets error on failure', async () => {
    mocks.checkForUpdate.mockRejectedValue(new Error('offline'));
    await updatesStore.check();
    expect(updatesStore.error).toBe('offline');
    expect(updatesStore.checking).toBe(false);
  });

  it('non-silent check clears a previous error before probing', async () => {
    updatesStore.error = 'old failure';
    mocks.checkForUpdate.mockResolvedValue(UP_TO_DATE);
    await updatesStore.check();
    expect(updatesStore.error).toBeNull();
  });

  it('ignores a call while a check is already running', async () => {
    let finish: (value: UpdateInfo) => void = () => {};
    mocks.checkForUpdate.mockReturnValue(new Promise<UpdateInfo>((resolve) => (finish = resolve)));
    const first = updatesStore.check();
    await updatesStore.check();
    expect(mocks.checkForUpdate).toHaveBeenCalledTimes(1);
    finish(AVAILABLE);
    await first;
  });
});

describe('updatesStore.pending', () => {
  it('is set only when an update is available', async () => {
    mocks.checkForUpdate.mockResolvedValue(UP_TO_DATE);
    await updatesStore.check();
    expect(updatesStore.pending).toBeNull();

    mocks.checkForUpdate.mockResolvedValue(AVAILABLE);
    await updatesStore.check();
    expect(updatesStore.pending).toEqual(AVAILABLE);
  });
});

describe('updatesStore.dismiss', () => {
  it('hides the offered version', async () => {
    mocks.checkForUpdate.mockResolvedValue(AVAILABLE);
    await updatesStore.check();
    updatesStore.dismiss();
    expect(updatesStore.dismissedVersion).toBe('1.2.0');
    expect(updatesStore.pending).toBeNull();
  });

  it('shows a later version again', async () => {
    mocks.checkForUpdate.mockResolvedValue(AVAILABLE);
    await updatesStore.check();
    updatesStore.dismiss();

    const newer = { ...AVAILABLE, version: '1.3.0' };
    mocks.checkForUpdate.mockResolvedValue(newer);
    await updatesStore.check();
    expect(updatesStore.pending).toEqual(newer);
  });

  it('does nothing without an update', () => {
    updatesStore.dismiss();
    expect(updatesStore.dismissedVersion).toBeNull();
  });
});

describe('updatesStore.install', () => {
  it('opens the release page instead of installing when can_install is false', async () => {
    mocks.checkForUpdate.mockResolvedValue({ ...AVAILABLE, can_install: false });
    await updatesStore.check();
    await updatesStore.install();
    expect(mocks.openExternal).toHaveBeenCalledWith(RELEASE_URL);
    expect(mocks.installUpdate).not.toHaveBeenCalled();
    expect(updatesStore.installing).toBe(false);
  });

  it('installs in place on desktop and marks installing while it runs', async () => {
    mocks.checkForUpdate.mockResolvedValue(AVAILABLE);
    await updatesStore.check();

    let finish: () => void = () => {};
    mocks.installUpdate.mockReturnValue(new Promise<void>((resolve) => (finish = resolve)));
    const pending = updatesStore.install();
    expect(updatesStore.installing).toBe(true);
    finish();
    await pending;

    expect(mocks.installUpdate).toHaveBeenCalledTimes(1);
    expect(mocks.openExternal).not.toHaveBeenCalled();
    expect(updatesStore.installing).toBe(false);
  });

  it('surfaces a string rejection from the backend as error', async () => {
    mocks.checkForUpdate.mockResolvedValue(AVAILABLE);
    await updatesStore.check();
    mocks.installUpdate.mockRejectedValue('signature mismatch');
    await updatesStore.install();
    expect(updatesStore.error).toBe('signature mismatch');
    expect(updatesStore.installing).toBe(false);
  });

  it('does nothing without an available update', async () => {
    await updatesStore.install();
    expect(mocks.installUpdate).not.toHaveBeenCalled();
    expect(mocks.openExternal).not.toHaveBeenCalled();
  });
});
