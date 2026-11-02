/**
 * Focus targets for the phone shell. The phone layout swaps the room list and the conversation
 * with `display: none`, so whichever pane stays on screen has to receive focus explicitly: without
 * this, focus falls back to `<body>` when the previously focused row is hidden.
 */

/** Marks the element that takes focus when a phone opens a conversation (the room heading). */
export const CHAT_ENTRY_ATTR = 'data-chat-entry';

/** Escape a value for use inside a double-quoted CSS attribute selector. */
export function escapeAttributeValue(value: string): string {
  return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

/** Selector for a room row by its room id, as rendered by the sidebar. */
export function roomRowSelector(roomId: string): string {
  return `[data-room-id="${escapeAttributeValue(roomId)}"]`;
}

/** Focus the first element matching `selector` under `root`. Returns whether focus moved. */
export function focusFirst(root: ParentNode, selector: string): boolean {
  const target = root.querySelector<HTMLElement>(selector);
  if (!target) return false;
  target.focus();
  return true;
}

/** Focus the room heading after a phone opens the conversation. */
export function focusChatEntry(root: ParentNode = document): boolean {
  return focusFirst(root, `[${CHAT_ENTRY_ATTR}]`);
}

/** Put focus back on the room row a phone chat was opened from. */
export function focusRoomRow(roomId: string, root: ParentNode = document): boolean {
  return focusFirst(root, roomRowSelector(roomId));
}

/**
 * Focus the room list for a skip link: the open (or last focused) row when one is rendered, else
 * the list container itself.
 */
export function focusRoomList(root: Document = document): boolean {
  const list = root.getElementById('room-list');
  if (!list) return false;
  const row =
    list.querySelector<HTMLElement>('button[data-room-id][aria-current="true"]') ??
    list.querySelector<HTMLElement>('button[data-room-id][tabindex="0"]') ??
    list.querySelector<HTMLElement>('button[data-room-id]');
  if (row) {
    row.focus();
    return true;
  }
  if (list instanceof HTMLElement) {
    if (!list.hasAttribute('tabindex')) list.setAttribute('tabindex', '-1');
    list.focus();
    return true;
  }
  return false;
}

/**
 * Move focus to a skip-link target by id, making a non-interactive landmark programmatically
 * focusable first (`tabindex="-1"`). Returns whether the target exists and took focus.
 */
export function focusSkipTarget(id: string, root: Document = document): boolean {
  const target = root.getElementById(id);
  if (!target) return false;
  if (!target.hasAttribute('tabindex')) target.setAttribute('tabindex', '-1');
  target.focus();
  return true;
}
