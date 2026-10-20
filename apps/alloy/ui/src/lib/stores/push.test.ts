import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { PushStore } from './push.svelte';
import * as pushPlatform from '../platform/push';

describe('PushStore transitions', () => {
  let store: PushStore;

  beforeEach(() => {
    vi.restoreAllMocks();
    store = new PushStore({
      supported: true,
      enabled: false,
      registered: false,
      distributor: null,
      distributors: [],
      endpoint: null,
      gateway: null,
      permission: 'unknown',
      error: null,
    });
  });

  afterEach(() => {
    store.stop();
  });

  it('initializes with given state', () => {
    expect(store.supported).toBe(true);
    expect(store.enabled).toBe(false);
    expect(store.registered).toBe(false);
    expect(store.distributor).toBe(null);
    expect(store.distributors).toEqual([]);
    expect(store.busy).toBe(false);
  });

  it('transitions state correctly on applyState', () => {
    store.busy = true;

    store.applyState({
      supported: true,
      enabled: true,
      registered: false,
      distributor: 'org.unifiedpush.distributor',
      distributors: ['org.unifiedpush.distributor'],
      endpoint: null,
      gateway: null,
      permission: 'granted',
      error: null,
    });

    expect(store.busy).toBe(false);
    expect(store.enabled).toBe(true);
    expect(store.registered).toBe(false);
    expect(store.distributor).toBe('org.unifiedpush.distributor');
    expect(store.permission).toBe('granted');
  });

  it('transitions to registered when endpoint is set', () => {
    store.applyState({
      supported: true,
      enabled: true,
      registered: true,
      distributor: 'org.unifiedpush.distributor',
      distributors: ['org.unifiedpush.distributor'],
      endpoint: 'https://push.example.com/endpoint',
      gateway: 'https://gateway.example.com',
      permission: 'granted',
      error: null,
    });

    expect(store.registered).toBe(true);
    expect(store.endpoint).toBe('https://push.example.com/endpoint');
    expect(store.gateway).toBe('https://gateway.example.com');
  });

  it('sets error on registration failure', () => {
    store.applyState({
      supported: true,
      enabled: true,
      registered: false,
      distributor: 'org.unifiedpush.distributor',
      distributors: ['org.unifiedpush.distributor'],
      endpoint: null,
      gateway: null,
      permission: 'denied',
      error: 'Permission denied for notifications',
    });

    expect(store.error).toBe('Permission denied for notifications');
    expect(store.permission).toBe('denied');
    expect(store.registered).toBe(false);
  });

  it('calls enablePush when toggled on and sets busy', () => {
    const enableSpy = vi.spyOn(pushPlatform, 'enablePush').mockImplementation(() => {});
    store.toggle(true);
    expect(store.busy).toBe(true);
    expect(enableSpy).toHaveBeenCalledOnce();
  });

  it('calls disablePush when toggled off and sets busy', () => {
    const disableSpy = vi.spyOn(pushPlatform, 'disablePush').mockImplementation(() => {});
    store.toggle(false);
    expect(store.busy).toBe(true);
    expect(disableSpy).toHaveBeenCalledOnce();
  });

  it('calls pickPushApp on pick()', () => {
    const pickSpy = vi.spyOn(pushPlatform, 'pickPushApp').mockImplementation(() => {});
    store.pick();
    expect(pickSpy).toHaveBeenCalledOnce();
  });

  it('disableOnSignOut returns immediately if push is not enabled', async () => {
    const disableSpy = vi.spyOn(pushPlatform, 'disablePush').mockImplementation(() => {});
    await store.disableOnSignOut(500);
    expect(disableSpy).not.toHaveBeenCalled();
  });

  it('disableOnSignOut calls disable and resolves when state changes to disabled', async () => {
    vi.useFakeTimers();
    try {
      store.applyState({
        supported: true,
        enabled: true,
        registered: true,
        distributor: 'ntfy',
        distributors: ['ntfy'],
        endpoint: 'https://example.com',
        gateway: null,
        permission: 'granted',
        error: null,
      });

      const disableSpy = vi.spyOn(pushPlatform, 'disablePush').mockImplementation(() => {
        queueMicrotask(() => {
          store.applyState({
            ...store.state,
            enabled: false,
            registered: false,
          });
        });
      });

      const promise = store.disableOnSignOut(1000);
      vi.runAllTimers();
      await promise;

      expect(disableSpy).toHaveBeenCalledOnce();
      expect(store.enabled).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('disableOnSignOut times out safely without throwing if bridge hangs', async () => {
    vi.useFakeTimers();
    try {
      store.applyState({
        supported: true,
        enabled: true,
        registered: true,
        distributor: 'ntfy',
        distributors: ['ntfy'],
        endpoint: 'https://example.com',
        gateway: null,
        permission: 'granted',
        error: null,
      });

      vi.spyOn(pushPlatform, 'disablePush').mockImplementation(() => {
        // Never transitions state
      });

      const promise = store.disableOnSignOut(50);
      vi.advanceTimersByTime(100);
      await promise;

      expect(store.enabled).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});
