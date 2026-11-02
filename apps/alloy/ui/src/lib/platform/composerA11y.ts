/**
 * ARIA for the composer's message field. The mention picker is a listbox popup owned by the field,
 * so while it is open the field is a full combobox (expanded, controls, active descendant). With
 * no popup open the field must stay a plain multi-line text field: a permanently collapsed
 * `role="combobox"` makes a screen reader announce every message as a combo box.
 */

export interface ComposerMentionState {
  /** Whether the mention listbox is showing. */
  open: boolean;
  /** DOM id of the mention listbox. */
  pickerId: string;
  /** Id of the option the keyboard has highlighted, when one is. */
  activeOptionId?: string | null;
}

/** Attributes spread on the composer textarea; empty when no mention popup is open. */
export interface ComposerFieldAria {
  role?: 'combobox';
  'aria-autocomplete'?: 'list';
  'aria-expanded'?: 'true';
  'aria-haspopup'?: 'listbox';
  'aria-controls'?: string;
  'aria-activedescendant'?: string;
}

/**
 * Computes the field's ARIA state. With the picker closed it returns no attributes at all, which
 * leaves the textarea's native textbox semantics (`<textarea>` is announced as a multi-line text
 * field). With the picker open it returns the complete combobox contract: the listbox is owned
 * through `aria-controls`, expanded through `aria-expanded`, and the highlighted option is the
 * active descendant.
 */
export function composerFieldAria(state: ComposerMentionState): ComposerFieldAria {
  if (!state.open || state.pickerId === '') return {};
  const aria: ComposerFieldAria = {
    role: 'combobox',
    'aria-autocomplete': 'list',
    'aria-expanded': 'true',
    'aria-haspopup': 'listbox',
    'aria-controls': state.pickerId,
  };
  const active = state.activeOptionId?.trim();
  if (active) aria['aria-activedescendant'] = active;
  return aria;
}
