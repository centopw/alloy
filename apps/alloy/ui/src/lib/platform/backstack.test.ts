import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/** Minimal window/history: back() pops one entry and fires popstate asynchronously like a browser. */
function installBrowser(width: number) {
  const listeners = new Set<() => void>();
  let entries = 1;
  const history = {
    pushState: vi.fn(() => {
      entries++;
    }),
    back: vi.fn(() => {
      if (entries > 1) entries--;
      queueMicrotask(() => listeners.forEach((l) => l()));
    }),
  };
  vi.stubGlobal('window', {
    innerWidth: width,
    addEventListener: (_: string, l: () => void) => listeners.add(l),
  });
  vi.stubGlobal('history', history);
  return {
    history,
    /** The user presses the system Back button. */
    pressBack: async () => {
      history.back();
      await Promise.resolve();
    },
    entries: () => entries,
  };
}

describe('backstack', () => {
  beforeEach(() => {
    vi.resetModules();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('does nothing on wide layouts', async () => {
    const env = installBrowser(1280);
    const { pushBack } = await import('./backstack');
    const release = pushBack(() => {});
    expect(env.history.pushState).not.toHaveBeenCalled();
    release();
    expect(env.history.back).not.toHaveBeenCalled();
  });

  it('Back closes only the topmost overlay, once', async () => {
    const env = installBrowser(400);
    const { pushBack } = await import('./backstack');
    const closed: string[] = [];
    pushBack(() => closed.push('chat'));
    pushBack(() => closed.push('settings'));
    await env.pressBack();
    expect(closed).toEqual(['settings']);
    await env.pressBack();
    expect(closed).toEqual(['settings', 'chat']);
    await env.pressBack();
    expect(closed).toEqual(['settings', 'chat']);
  });

  it('closing programmatically removes the history entry without running the handler', async () => {
    const env = installBrowser(400);
    const { pushBack } = await import('./backstack');
    const handler = vi.fn();
    const release = pushBack(handler);
    expect(env.entries()).toBe(2);
    release();
    await Promise.resolve();
    expect(env.entries()).toBe(1);
    expect(handler).not.toHaveBeenCalled();
    await env.pressBack();
    expect(handler).not.toHaveBeenCalled();
  });
});
