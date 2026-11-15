import { describe, expect, it } from 'vitest';
import { titlebarMode } from './titlebar';

describe('titlebarMode', () => {
  it('uses the overlay bar only in the macOS Tauri shell', () => {
    expect(titlebarMode(true, 'MacIntel')).toBe('overlay');
    expect(titlebarMode(true, 'macOS')).toBe('overlay');
  });

  it('keeps the native bar in the macOS browser build', () => {
    expect(titlebarMode(false, 'MacIntel')).toBe('native');
  });

  it('keeps the native bar on Windows, Linux and Android shells', () => {
    expect(titlebarMode(true, 'Win32')).toBe('native');
    expect(titlebarMode(true, 'Linux x86_64')).toBe('native');
    expect(titlebarMode(true, 'Android')).toBe('native');
  });
});
