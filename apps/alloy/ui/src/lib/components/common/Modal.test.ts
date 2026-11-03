import { afterEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import ConfirmDialog from './ConfirmDialog.svelte';

/**
 * A dialog can be opened from a pane that scrolls and clips: the room list and the conversation are
 * `overflow: hidden` and carry a backdrop-filter, which is both the clipping box and the containing
 * block for `position: fixed`. Before the portal, leaving a room from the room list rendered the
 * confirmation clipped to the sidebar (the "Leave room" dialog was cut off halfway through).
 */
function renderInsideClippedPane() {
  const pane = document.createElement('div');
  pane.style.overflow = 'hidden';
  pane.style.backdropFilter = 'blur(1px)';
  document.body.appendChild(pane);
  const component = mount(ConfirmDialog, {
    target: pane,
    props: {
      open: true,
      title: 'Leave room',
      message: 'You will leave this room.',
      confirmLabel: 'Leave',
      danger: true,
      onconfirm: () => {},
      onclose: () => {}
    }
  });
  flushSync();
  return { pane, component };
}

afterEach(() => {
  document.body.replaceChildren();
});

describe('Modal portal', () => {
  it('renders the dialog on the body, not inside the clipping pane that opened it', () => {
    const { pane } = renderInsideClippedPane();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog, 'the dialog is rendered').toBeTruthy();
    expect(pane.querySelector('[role="dialog"]'), 'the dialog escaped the pane').toBeNull();
    const backdrop = dialog!.closest('.backdrop');
    expect(backdrop?.parentElement, 'the overlay lives on the body').toBe(document.body);
  });

  it('removes the portalled overlay when the dialog closes', () => {
    const { component } = renderInsideClippedPane();
    unmount(component);
    flushSync();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
});
