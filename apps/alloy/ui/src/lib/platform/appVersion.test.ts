import { beforeEach, describe, expect, it, vi } from 'vitest';

const shell = vi.hoisted(() => ({ isTauri: false, getVersion: vi.fn() }));

vi.mock('./notify', () => ({ isTauriShell: () => shell.isTauri }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: shell.getVersion }));

import { appVersion, WEB_VERSION } from './appVersion';

describe('appVersion', () => {
  beforeEach(() => {
    shell.getVersion.mockReset();
  });

  it('asks the Tauri shell for its version', async () => {
    shell.isTauri = true;
    shell.getVersion.mockResolvedValue('0.2.2');
    await expect(appVersion()).resolves.toBe('0.2.2');
  });

  it('does not touch the shell API outside Tauri', async () => {
    shell.isTauri = false;
    await expect(appVersion()).resolves.toBe(WEB_VERSION);
    expect(shell.getVersion).not.toHaveBeenCalled();
  });
});
