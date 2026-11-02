import { describe, expect, it } from 'vitest';
import { composerFieldAria } from './composerA11y';

describe('composerFieldAria', () => {
  it('leaves the field a plain multi-line text field with no mention popup', () => {
    expect(composerFieldAria({ open: false, pickerId: 'mentions-1' })).toEqual({});
  });

  it('implements the full combobox contract while the popup is open', () => {
    expect(
      composerFieldAria({
        open: true,
        pickerId: 'mentions-1',
        activeOptionId: 'mentions-1-option-2',
      })
    ).toEqual({
      role: 'combobox',
      'aria-autocomplete': 'list',
      'aria-expanded': 'true',
      'aria-haspopup': 'listbox',
      'aria-controls': 'mentions-1',
      'aria-activedescendant': 'mentions-1-option-2',
    });
  });

  it('omits the active descendant when no option is highlighted', () => {
    const aria = composerFieldAria({ open: true, pickerId: 'mentions-1', activeOptionId: null });
    expect(aria['aria-activedescendant']).toBeUndefined();
    expect(aria['aria-controls']).toBe('mentions-1');
  });

  it('omits a blank active descendant id', () => {
    expect(
      composerFieldAria({ open: true, pickerId: 'mentions-1', activeOptionId: '   ' })[
        'aria-activedescendant'
      ]
    ).toBeUndefined();
  });

  it('never builds an empty listbox contract', () => {
    expect(composerFieldAria({ open: true, pickerId: '' })).toEqual({});
  });
});
