import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type * as PermissionsModule from '../platform/permissions';
import type { PermissionId, PermissionState } from '../platform/permissions';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

const platform = vi.hoisted(() => ({
  android: false,
  states: new Map<string, string>(),
  readPermission: vi.fn(),
  requestPermission: vi.fn(),
}));

vi.mock('../platform/capabilities', () => ({ isAndroid: () => platform.android }));
vi.mock('./push.svelte', () => ({
  pushStore: { supported: false, distributors: [], registered: false, pick: vi.fn() },
}));
vi.mock('../platform/permissions', async (importOriginal) => {
  const actual = await importOriginal<typeof PermissionsModule>();
  return {
    ...actual,
    readPermission: (...args: unknown[]) => platform.readPermission(...args),
    requestPermission: (...args: unknown[]) => platform.requestPermission(...args),
  };
});
import { permissionsStore } from './permissions.svelte';
import { supportsPermissionSettings } from '../platform/permissions';

function stateOf(id: PermissionId): PermissionState {
  const row = permissionsStore.rows.find((r) => r.id === id);
  if (!row) throw new Error(`no ${id} row`);
  return row.state;
}

const host = window as unknown as {
  __TAURI_INTERNALS__?: object;
  AlloyPermissions?: {
    status: (name: string) => string;
    request: (name: string) => void;
    openSettings: (name: string) => void;
  };
};

beforeEach(() => {
  host.__TAURI_INTERNALS__ = {};
  delete host.AlloyPermissions;
  platform.android = false;
  platform.states = new Map([
    ['notifications', 'prompt'],
    ['microphone', 'prompt'],
    ['location', 'prompt'],
  ]);
  invoke.mockReset().mockResolvedValue(undefined);
  platform.readPermission.mockReset().mockImplementation(async (id: string) => platform.states.get(id) ?? 'unknown');
  platform.requestPermission.mockReset().mockImplementation(async (id: string) => {
    platform.states.set(id, 'granted');
  });
});

afterEach(() => {
  delete host.__TAURI_INTERNALS__;
  delete host.AlloyPermissions;
  vi.unstubAllGlobals();
});

describe('permissionsStore', () => {
  it('reads every state without asking anything', async () => {
    await permissionsStore.refresh();
    expect(permissionsStore.loaded).toBe(true);
    expect(stateOf('notifications')).toBe('prompt');
    expect(platform.requestPermission).not.toHaveBeenCalled();
  });

  it('shows a permission as allowed right after the user allows it, and the summary follows', async () => {
    await permissionsStore.refresh();
    expect(permissionsStore.summary.missingRequired).toBe(1);

    await permissionsStore.request('notifications');

    expect(stateOf('notifications')).toBe('granted');
    expect(permissionsStore.summary.missingRequired).toBe(0);
  });

  it('picks up a change made in the system settings when the window gets focus back', async () => {
    platform.states.set('notifications', 'denied');
    const stop = permissionsStore.watch();
    await vi.waitFor(() => expect(stateOf('notifications')).toBe('denied'));

    platform.states.set('notifications', 'granted');
    window.dispatchEvent(new Event('focus'));

    await vi.waitFor(() => expect(stateOf('notifications')).toBe('granted'));
    stop();
  });

  it('picks up the Android shell answer event', async () => {
    const stop = permissionsStore.watch();
    await vi.waitFor(() => expect(permissionsStore.loaded).toBe(true));

    platform.states.set('microphone', 'granted');
    window.dispatchEvent(new CustomEvent('alloy-permission-changed', { detail: { name: 'microphone' } }));

    await vi.waitFor(() => expect(stateOf('microphone')).toBe('granted'));
    stop();
  });

  it('stops listening when the last screen goes away', async () => {
    const stop = permissionsStore.watch();
    await vi.waitFor(() => expect(permissionsStore.loaded).toBe(true));
    stop();
    platform.readPermission.mockClear();

    window.dispatchEvent(new Event('focus'));
    await Promise.resolve();

    expect(platform.readPermission).not.toHaveBeenCalled();
  });

  it('lists the Android only permissions only on Android, and offers the system page only with the native bridge', async () => {
    expect(permissionsStore.rows.map((r) => r.id)).not.toContain('battery');

    platform.android = true;
    platform.states.set('battery', 'prompt');
    await permissionsStore.refresh();

    expect(permissionsStore.rows.map((r) => r.id)).toContain('battery');
    // Without the bridge, Android cannot open settings
    expect(permissionsStore.rows.every((r) => r.canOpenSettings)).toBe(false);

    // With the native bridge, settings can be opened
    const bridge = {
      status: vi.fn().mockReturnValue('prompt'),
      request: vi.fn(),
      openSettings: vi.fn(),
    };
    host.AlloyPermissions = bridge;

    expect(permissionsStore.rows.every((r) => r.canOpenSettings)).toBe(true);

    const opened = await permissionsStore.openSettings('push-app');
    expect(opened).toBe(true);
    expect(bridge.openSettings).toHaveBeenCalledWith('notifications');
  });

  it('enables opening settings on desktop Tauri and calls open_permission_settings', async () => {
    expect(permissionsStore.rows.every((r) => r.canOpenSettings)).toBe(true);

    const opened = await permissionsStore.openSettings('notifications');
    expect(opened).toBe(true);
    expect(invoke).toHaveBeenCalledWith('open_permission_settings', { permission: 'notifications' });
  });

  it('refuses unsupported permissions on desktop Tauri and handles backend errors gracefully', async () => {
    const openedBattery = await permissionsStore.openSettings('battery');
    expect(openedBattery).toBe(false);
    expect(invoke).not.toHaveBeenCalled();

    invoke.mockRejectedValueOnce(new Error('Settings app failed'));
    const openedError = await permissionsStore.openSettings('microphone');
    expect(openedError).toBe(false);
    expect(invoke).toHaveBeenCalledWith('open_permission_settings', { permission: 'microphone' });
  });

  it('disables opening settings and returns false in a plain browser', async () => {
    delete host.__TAURI_INTERNALS__;

    expect(permissionsStore.rows.every((r) => r.canOpenSettings)).toBe(false);

    const opened = await permissionsStore.openSettings('notifications');
    expect(opened).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('keeps a failed read from breaking the screen', async () => {
    platform.readPermission.mockImplementation(async (id: string) => {
      if (id === 'location') throw new Error('boom');
      return 'granted';
    });
    await permissionsStore.refresh();

    expect(stateOf('location')).toBe('unknown');
    expect(stateOf('notifications')).toBe('granted');
  });
});

describe('supportsPermissionSettings', () => {
  it('returns true on desktop Tauri for desktop permissions and false for Android-only', () => {
    expect(supportsPermissionSettings()).toBe(true);
    expect(supportsPermissionSettings('notifications')).toBe(true);
    expect(supportsPermissionSettings('microphone')).toBe(true);
    expect(supportsPermissionSettings('location')).toBe(true);
    expect(supportsPermissionSettings('battery')).toBe(false);
    expect(supportsPermissionSettings('push-app')).toBe(false);
  });

  it('returns false in a plain browser', () => {
    delete host.__TAURI_INTERNALS__;
    expect(supportsPermissionSettings()).toBe(false);
    expect(supportsPermissionSettings('notifications')).toBe(false);
    expect(supportsPermissionSettings('microphone')).toBe(false);
  });

  it('returns true on Android only when the native bridge is mounted', () => {
    platform.android = true;
    expect(supportsPermissionSettings('notifications')).toBe(false);

    host.AlloyPermissions = { status: vi.fn(), request: vi.fn(), openSettings: vi.fn() };
    expect(supportsPermissionSettings('notifications')).toBe(true);
    expect(supportsPermissionSettings('battery')).toBe(true);
  });
});
