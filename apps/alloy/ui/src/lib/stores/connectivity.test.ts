import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { connectivity, PROBE_INTERVAL_MS } from './connectivity.svelte';
import { errorMessage, isNetworkError, OFFLINE_MESSAGE } from './errors';

const RAW =
  'error sending request for url (https://matrix.hce.vn/_matrix/client/v3/user/@cent:hce.vn/account_data/chat.alloy.prefs)';

describe('network errors', () => {
  it('recognises transport failures from the SDK and the browser', () => {
    expect(isNetworkError({ code: 'network_error', message: RAW })).toBe(true);
    expect(isNetworkError({ message: 'dns error: failed to lookup address information' })).toBe(true);
    expect(isNetworkError(new TypeError('Failed to fetch'))).toBe(true);
  });

  it('does not treat a refusal from the server as being offline', () => {
    expect(isNetworkError({ code: 'network_error', message: 'M_FORBIDDEN: You are not allowed to do that' })).toBe(false);
    expect(isNetworkError({ message: 'The room does not exist' })).toBe(false);
  });

  it('shows a plain sentence, never the request URL', () => {
    expect(errorMessage({ code: 'network_error', message: RAW })).toBe(OFFLINE_MESSAGE);
    expect(errorMessage({ message: 'Room not found' })).toBe('Room not found');
    expect(errorMessage(undefined)).toBe('Something went wrong');
  });
});

describe('connectivity', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    connectivity.reset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('goes offline on a network failure but ignores other failures', () => {
    connectivity.noteFailure({ message: 'M_FORBIDDEN' });
    expect(connectivity.offline).toBe(false);
    connectivity.noteFailure({ message: RAW });
    expect(connectivity.offline).toBe(true);
  });

  it('probes while offline, comes back when the probe succeeds and runs the reconnect hooks once', async () => {
    const probe = vi.fn().mockRejectedValueOnce({ message: RAW }).mockResolvedValue(undefined);
    const hook = vi.fn();
    const stop = connectivity.start(probe);
    const unhook = connectivity.onReconnect(hook);

    connectivity.noteFailure({ message: RAW });
    await vi.advanceTimersByTimeAsync(PROBE_INTERVAL_MS);
    expect(connectivity.offline).toBe(true);
    expect(hook).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(PROBE_INTERVAL_MS);
    expect(connectivity.offline).toBe(false);
    expect(hook).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(PROBE_INTERVAL_MS * 3);
    expect(probe).toHaveBeenCalledTimes(2);
    unhook();
    stop();
  });

  it('follows the browser online and offline events', async () => {
    const hook = vi.fn();
    const stop = connectivity.start(() => Promise.resolve());
    connectivity.onReconnect(hook);

    window.dispatchEvent(new Event('offline'));
    expect(connectivity.offline).toBe(true);
    window.dispatchEvent(new Event('online'));
    await vi.advanceTimersByTimeAsync(0);
    expect(connectivity.offline).toBe(false);
    expect(hook).toHaveBeenCalledTimes(1);
    stop();
  });
});
