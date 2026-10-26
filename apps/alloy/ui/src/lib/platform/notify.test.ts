import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

import { requestNotificationPermission, showNotification } from './notify';

const host = window as unknown as { __TAURI_INTERNALS__?: object };

beforeEach(() => {
  invoke.mockReset();
});

afterEach(() => {
  delete host.__TAURI_INTERNALS__;
  vi.unstubAllGlobals();
});

describe('requestNotificationPermission', () => {
  it('asks the shell when running inside Tauri', async () => {
    host.__TAURI_INTERNALS__ = {};
    invoke.mockResolvedValue(true);
    await expect(requestNotificationPermission()).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith('notify_permission');
  });

  it('reports denied in the browser when the Notification API is missing', async () => {
    vi.stubGlobal('Notification', undefined);
    await expect(requestNotificationPermission()).resolves.toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('asks the browser for permission when it has not been decided', async () => {
    const requestPermission = vi.fn().mockResolvedValue('granted');
    vi.stubGlobal('Notification', Object.assign(vi.fn(), { permission: 'default', requestPermission }));
    await expect(requestNotificationPermission()).resolves.toBe(true);
    expect(requestPermission).toHaveBeenCalledOnce();
  });
});

describe('showNotification', () => {
  it('sends title, body and silent flag to the shell', async () => {
    host.__TAURI_INTERNALS__ = {};
    invoke.mockResolvedValue(undefined);
    await showNotification({ title: 'Room', body: 'hi', silent: true, roomId: '!room:alloy.chat' });
    expect(invoke).toHaveBeenCalledWith('notify_message', {
      title: 'Room',
      body: 'hi',
      silent: true,
      roomId: '!room:alloy.chat',
    });
  });

  it('rejects in the browser when permission is not granted', async () => {
    vi.stubGlobal('Notification', Object.assign(vi.fn(), { permission: 'denied' }));
    await expect(
      showNotification({ title: 'Room', body: 'hi', silent: false, roomId: '!room:alloy.chat' })
    ).rejects.toThrow(
      'permission is not granted'
    );
  });
});
