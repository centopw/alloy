import { afterEach, describe, expect, it, vi } from 'vitest';
import type { UpdateInfo, UpdateProgress } from './updates';

const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listen(...args),
}));

import { applyUpdate, checkForUpdate, downloadUpdate, onUpdateDownloaded } from './updates';

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
  downloaded: false,
};

afterEach(() => {
  setTauri(false);
  invoke.mockReset();
  listen.mockReset();
});

describe('updates platform wrappers outside Tauri', () => {
  it('check resolves null without invoking the backend', async () => {
    expect(await checkForUpdate()).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('download and apply are no-ops', async () => {
    await expect(downloadUpdate(() => {})).resolves.toBeUndefined();
    await expect(applyUpdate()).resolves.toBeUndefined();
    expect(invoke).not.toHaveBeenCalled();
    expect(listen).not.toHaveBeenCalled();
  });

  it('downloaded subscription returns a teardown and never listens', () => {
    const off = onUpdateDownloaded(() => {});
    expect(() => off()).not.toThrow();
    expect(listen).not.toHaveBeenCalled();
  });
});

describe('updates platform wrappers inside Tauri', () => {
  it('check invokes check_update and returns the info', async () => {
    setTauri(true);
    invoke.mockResolvedValueOnce(INFO);
    expect(await checkForUpdate()).toEqual(INFO);
    expect(invoke).toHaveBeenCalledWith('check_update');
  });

  it('download forwards progress events and removes the listener afterwards', async () => {
    setTauri(true);
    const unlisten = vi.fn();
    let handler: ((event: { payload: UpdateProgress }) => void) | undefined;
    listen.mockImplementationOnce(async (_name: string, cb: typeof handler) => {
      handler = cb;
      return unlisten;
    });
    invoke.mockImplementationOnce(async () => {
      handler?.({ payload: { downloaded: 5, total: 10 } });
    });

    const seen: UpdateProgress[] = [];
    await downloadUpdate((p) => seen.push(p));

    expect(listen).toHaveBeenCalledWith('update://progress', expect.any(Function));
    expect(invoke).toHaveBeenCalledWith('download_update');
    expect(seen).toEqual([{ downloaded: 5, total: 10 }]);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('download removes the listener even when the backend rejects', async () => {
    setTauri(true);
    const unlisten = vi.fn();
    listen.mockResolvedValueOnce(unlisten);
    invoke.mockRejectedValueOnce('signature mismatch');

    await expect(downloadUpdate(() => {})).rejects.toBe('signature mismatch');
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('apply invokes apply_update', async () => {
    setTauri(true);
    invoke.mockResolvedValueOnce(undefined);
    await applyUpdate();
    expect(invoke).toHaveBeenCalledWith('apply_update');
  });

  it('downloaded subscription forwards the event and tears down', async () => {
    setTauri(true);
    const unlisten = vi.fn();
    let handler: (() => void) | undefined;
    listen.mockImplementationOnce(async (_name: string, cb: () => void) => {
      handler = cb;
      return unlisten;
    });

    const cb = vi.fn();
    const off = onUpdateDownloaded(cb);
    await vi.waitFor(() => expect(listen).toHaveBeenCalledWith('update://downloaded', expect.any(Function)));
    handler?.();
    expect(cb).toHaveBeenCalledTimes(1);

    off();
    await vi.waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });
});
