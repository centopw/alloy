import { describe, expect, it } from 'vitest';
import { modKey, shortcutLabel } from './shortcuts';

describe('modKey', () => {
  it('uses Cmd on Apple platforms', () => {
    expect(modKey('MacIntel')).toBe('Cmd');
    expect(modKey('Macintosh')).toBe('Cmd');
    expect(modKey('macOS')).toBe('Cmd');
    expect(modKey('iPhone')).toBe('Cmd');
    expect(modKey('iPad')).toBe('Cmd');
  });

  it('uses Ctrl on Windows, Linux and Android', () => {
    expect(modKey('Win32')).toBe('Ctrl');
    expect(modKey('Windows')).toBe('Ctrl');
    expect(modKey('Linux x86_64')).toBe('Ctrl');
    expect(modKey('Linux armv8l')).toBe('Ctrl');
    expect(modKey('Android')).toBe('Ctrl');
  });

  it('uses Ctrl when the platform is unknown', () => {
    expect(modKey('')).toBe('Ctrl');
  });
});

describe('shortcutLabel', () => {
  it('formats the modifier and key for the given platform', () => {
    expect(shortcutLabel('K', 'MacIntel')).toBe('Cmd K');
    expect(shortcutLabel('K', 'Win32')).toBe('Ctrl K');
  });

  it('falls back to the running platform when none is injected', () => {
    const label = shortcutLabel('K');
    expect(['Cmd K', 'Ctrl K']).toContain(label);
  });
});
