import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import UpdateNotice from './UpdateNotice.svelte';
import type { UpdateInfo } from '$lib/platform/updates';

const store = vi.hoisted(() => ({
  info: null as UpdateInfo | null,
  pending: null as UpdateInfo | null,
  phase: 'available' as string,
  busy: false,
  progress: null as number | null,
  primary: vi.fn(),
  remindLater: vi.fn(),
  openChangelog: vi.fn(),
}));

vi.mock('$lib/stores/updates.svelte', () => ({ updatesStore: store }));
vi.mock('$lib/platform/contextmenu', () => ({ contextmenu: () => {} }));

beforeAll(() => {
  // jsdom has no ResizeObserver; the bar only uses it to pick its narrow layout.
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

const teardown: Array<() => void> = [];

function render(info: UpdateInfo): HTMLElement {
  store.info = info;
  store.pending = info;
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(UpdateNotice, { target, props: {} });
  teardown.push(() => unmount(instance));
  flushSync();
  return target;
}

function update(install_method: UpdateInfo['install_method'], can_install: boolean): UpdateInfo {
  return {
    available: true,
    version: '0.5.0',
    notes: '## Fixes',
    can_install,
    url: 'https://example.org/releases/0.5.0',
    downloaded: false,
    install_method,
  };
}

afterEach(() => {
  for (const destroy of teardown.splice(0)) destroy();
  document.body.replaceChildren();
});

describe('UpdateNotice', () => {
  it('offers the download only to a manual copy', () => {
    const manual = render(update('manual', true));
    expect(manual.textContent).toContain('Download');

    const homebrew = render(update('homebrew', false));
    expect(homebrew.textContent).not.toMatch(/download|install|restart/i);
    expect(homebrew.textContent).toContain('Open');
  });

  it('never offers an install for a translocated copy', () => {
    const root = render(update('translocated', false));
    expect(root.textContent).not.toMatch(/download|install|restart/i);
  });
});
