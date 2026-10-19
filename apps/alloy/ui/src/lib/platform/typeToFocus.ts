/** Marks the composer's text field so a stray keystroke elsewhere can be redirected into it. */
export const COMPOSER_FIELD_ATTR = 'data-composer-field';

const EDITABLE = 'input, textarea, select, [contenteditable=""], [contenteditable="true"]';
/** Surfaces that own the keyboard while open: dialogs, menus, popovers. */
const MODAL = '[role="dialog"], [role="menu"], [role="listbox"], [role="combobox"][aria-expanded="true"]';

/**
 * Whether a keydown is plain typing that should land in the composer: a single printable character
 * with no shortcut modifier, aimed at nothing that takes text itself, outside any open dialog or menu.
 * Space is left alone on buttons and links, where it activates the control.
 */
export function shouldFocusComposer(e: KeyboardEvent): boolean {
  if (e.defaultPrevented || e.isComposing || e.metaKey || e.ctrlKey || e.altKey) return false;
  if (e.key.length !== 1) return false;
  const target = e.target;
  if (!(target instanceof Element)) return true;
  if (target.closest(EDITABLE) || target.closest(MODAL)) return false;
  if (e.key === ' ' && target.closest('button, a[href], [role="button"], summary')) return false;
  return true;
}

/** Move focus to the composer field if one is on screen. Returns whether it took focus. */
export function focusComposer(root: ParentNode = document): boolean {
  const field = root.querySelector<HTMLTextAreaElement>(`textarea[${COMPOSER_FIELD_ATTR}]`);
  if (!field) return false;
  field.focus();
  return true;
}
