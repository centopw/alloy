import { describe, expect, it } from 'vitest';
import { decideMenu, type NativeMenuEntry, type NativeMenuTarget } from './nativeMenu';

const NONE: NativeMenuTarget = { editable: null, link: null, hasTextSelection: false };

/** Compact shape: `id:on|off` for actions, `-` for separators. */
function shape(entries: NativeMenuEntry[]): string[] {
  return entries.map((e) => ('separator' in e ? '-' : `${e.id}:${e.enabled ? 'on' : 'off'}`));
}

describe('decideMenu', () => {
  it('shows nothing for an unhandled target with no selection', () => {
    expect(decideMenu(NONE)).toEqual([]);
  });

  it('offers full edit actions for a writable field with no selection', () => {
    const entries = decideMenu({ ...NONE, editable: { readonly: false, hasSelection: false } });
    expect(shape(entries)).toEqual([
      'undo:on',
      'redo:on',
      '-',
      'cut:off',
      'copy:off',
      'paste:on',
      '-',
      'selectAll:on',
    ]);
  });

  it('enables cut and copy for a writable field with a selection', () => {
    const entries = decideMenu({ ...NONE, editable: { readonly: false, hasSelection: true } });
    expect(shape(entries)).toEqual([
      'undo:on',
      'redo:on',
      '-',
      'cut:on',
      'copy:on',
      'paste:on',
      '-',
      'selectAll:on',
    ]);
  });

  it('disables every mutating action for a readonly field, keeping copy and select all', () => {
    const entries = decideMenu({ ...NONE, editable: { readonly: true, hasSelection: true } });
    expect(shape(entries)).toEqual([
      'undo:off',
      'redo:off',
      '-',
      'cut:off',
      'copy:on',
      'paste:off',
      '-',
      'selectAll:on',
    ]);
  });

  it('offers open and copy-address for a link', () => {
    expect(shape(decideMenu({ ...NONE, link: { href: 'https://example.org' } }))).toEqual([
      'openLink:on',
      'copyLink:on',
    ]);
  });

  it('appends copy after a separator when a link also has a text selection', () => {
    const entries = decideMenu({ ...NONE, link: { href: 'https://example.org' }, hasTextSelection: true });
    expect(shape(entries)).toEqual(['openLink:on', 'copyLink:on', '-', 'copy:on']);
  });

  it('offers only copy for a plain text selection', () => {
    expect(shape(decideMenu({ ...NONE, hasTextSelection: true }))).toEqual(['copy:on']);
  });

  it('gives editable priority over a link and a selection', () => {
    const entries = decideMenu({
      editable: { readonly: false, hasSelection: true },
      link: { href: 'https://example.org' },
      hasTextSelection: true,
    });
    expect(entries.some((e) => 'id' in e && e.id === 'openLink')).toBe(false);
    expect(shape(entries)[0]).toBe('undo:on');
  });
});
