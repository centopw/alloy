import { afterEach, describe, expect, it, vi } from 'vitest';
import { isChromeSessionSupported } from './chromeSession';

const host = window as unknown as { __TAURI_INTERNALS__?: object };

function setShell(on: boolean, agent = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0)') {
  if (on) host.__TAURI_INTERNALS__ = {};
  else delete host.__TAURI_INTERNALS__;
  vi.stubGlobal('navigator', { userAgent: agent });
}

afterEach(() => {
  delete host.__TAURI_INTERNALS__;
  vi.unstubAllGlobals();
});

describe('isChromeSessionSupported', () => {
  it('is offered by the desktop app on a Mac', () => {
    setShell(true);
    expect(isChromeSessionSupported()).toBe(true);
  });

  it('is not offered outside the shell, where there is no Chrome to read', () => {
    setShell(false);
    expect(isChromeSessionSupported()).toBe(false);
  });

  it('is not offered on Android or on a Windows Chrome', () => {
    setShell(true, 'Mozilla/5.0 (Linux; Android 15)');
    expect(isChromeSessionSupported()).toBe(false);
    setShell(true, 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)');
    expect(isChromeSessionSupported()).toBe(false);
  });
});
