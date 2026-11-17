import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import HelpAbout from './HelpAbout.svelte';
import type { InstallMethod, UpdateInfo } from '$lib/platform/updates';

const store = vi.hoisted(() => ({
  info: null as UpdateInfo | null,
  checking: false,
  busy: false,
  error: null as string | null,
  check: vi.fn(),
  openChangelog: vi.fn(),
}));
const copyText = vi.hoisted(() => vi.fn(async (_text: string) => {}));

vi.mock('$lib/stores/updates.svelte', () => ({ updatesStore: store }));
vi.mock('$lib/platform/clipboard', () => ({ copyText }));
vi.mock('$lib/platform/appVersion', () => ({ appVersion: async () => '0.4.18' }));
vi.mock('$lib/platform/capabilities', () => ({
  capabilities: () => ({ autoDownloadUpdates: false }),
  currentEnv: () => 'desktop',
}));
vi.mock('$lib/platform/links', () => ({ openExternal: vi.fn(async () => {}) }));
vi.mock('$lib/stores/prefs.svelte', () => ({
  prefs: { autoCheckUpdates: true, autoDownloadUpdates: false, set: vi.fn() },
}));
vi.mock('$lib/stores/ui.svelte', () => ({ ui: { hudVisible: false } }));
vi.mock('$lib/stores/viewport.svelte', () => ({ viewport: { isPhone: false } }));

const teardown: Array<() => void> = [];

function info(install_method: InstallMethod): UpdateInfo {
  return {
    available: true,
    version: '0.5.0',
    notes: '## Fixes',
    can_install: install_method === 'manual',
    url: 'https://example.org/releases/0.5.0',
    downloaded: false,
    install_method,
  };
}

function render(update: UpdateInfo | null): HTMLElement {
  store.info = update;
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(HelpAbout, { target, props: {} });
  teardown.push(() => unmount(instance));
  flushSync();
  return target;
}

function buttonLabels(root: HTMLElement): string[] {
  return [...root.querySelectorAll('button')].map((b) => b.textContent?.trim() ?? '');
}

afterEach(() => {
  for (const destroy of teardown.splice(0)) destroy();
  document.body.replaceChildren();
  vi.clearAllMocks();
});

describe('HelpAbout update surface', () => {
  it('tells a Homebrew copy the exact upgrade command instead of offering an install', () => {
    const root = render(info('homebrew'));

    expect(root.querySelector('code')?.textContent?.trim()).toBe(
      'brew upgrade --cask centopw/tap/alloy-client'
    );
    expect(root.textContent).toContain('brew tap centopw/tap');
    expect(root.textContent).toContain('brew trust --cask centopw/tap/alloy-client');
    expect(root.textContent?.toLowerCase()).toContain('homebrew installed this copy');

    const labels = buttonLabels(root);
    expect(labels).toContain('Check again');
    for (const label of labels) {
      expect(label.toLowerCase()).not.toMatch(/download|install|restart/);
    }
  });

  it('copies the upgrade command', async () => {
    const root = render(info('homebrew'));
    const copy = [...root.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Copy');

    copy?.click();
    await vi.waitFor(() =>
      expect(copyText).toHaveBeenCalledWith('brew upgrade --cask centopw/tap/alloy-client')
    );
  });

  it('tells a translocated copy to move the app first and offers no install', () => {
    const root = render(info('translocated'));

    expect(root.textContent).toContain('Move Alloy to Applications');
    expect(root.textContent).not.toContain('brew upgrade');
    for (const label of buttonLabels(root)) {
      expect(label.toLowerCase()).not.toMatch(/download|install|restart/);
    }
  });

  it('keeps the manual copy on the existing check-for-updates surface', () => {
    const root = render(info('manual'));

    expect(root.textContent).not.toContain('brew upgrade');
    expect(root.textContent).not.toContain('Move Alloy to Applications');
    expect(buttonLabels(root)).toContain('Check for updates');
    expect(buttonLabels(root)).not.toContain('Check again');
  });
});
