import { afterEach, describe, expect, it, vi } from 'vitest';
import type { UpdateInfo } from './updates';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

import { checkForUpdate, installUpdate } from './updates';

const host = window as unknown as { __TAURI_INTERNALS__?: object };

function setTauri(on: boolean) {
  if (on) host.__TAURI_INTERNALS__ = {};
  else delete host.__TAURI_INTERNALS__;
}

const INFO: UpdateInfo = {
  available: true,
  version: '1.2.0',
  notes: 'Fixes',
  can_install: true,
  url: 'https://example.org/releases/1.2.0',
};

afterEach(() => {
  setTauri(false);
  invoke.mockReset();
});

describe('updates platform wrappers', () => {
  it('check is a no-op returning null outside Tauri', async () => {
    expect(await checkForUpdate()).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('install is a no-op outside Tauri', async () => {
    await expect(installUpdate()).resolves.toBeUndefined();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('check invokes check_update under Tauri and returns its result', async () => {
    setTauri(true);
    invoke.mockResolvedValue(INFO);
    expect(await checkForUpdate()).toEqual(INFO);
    expect(invoke).toHaveBeenCalledWith('check_update');
  });

  it('install invokes install_update under Tauri', async () => {
    setTauri(true);
    invoke.mockResolvedValue(undefined);
    await installUpdate();
    expect(invoke).toHaveBeenCalledWith('install_update');
  });

  it('install propagates the rejection message from the backend', async () => {
    setTauri(true);
    invoke.mockRejectedValue('signature mismatch');
    await expect(installUpdate()).rejects.toBe('signature mismatch');
  });
});
