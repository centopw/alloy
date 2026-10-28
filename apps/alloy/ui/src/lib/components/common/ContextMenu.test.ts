import { afterEach, describe, expect, it } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ContextMenu from './ContextMenu.svelte';
import { contextMenu } from '$lib/stores/contextmenu.svelte';

/**
 * #111: on Android (adjustResize) the soft keyboard shrinks the WebView height and fires `resize`.
 * The menu used to close on any resize, so tapping the Tags search field killed the sheet before
 * the keyboard could open. These tests pin the difference between that and a real window resize.
 */

const SEARCH = 'Search or create a tag';

function render() {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const component = mount(ContextMenu, { target });
  return { target, component };
}

/** Opens a one-row menu whose child flyout shows the tag search field. */
async function openTagsMenuWithSearch() {
  contextMenu.open(
    [
      {
        id: 'tags',
        label: 'Tags',
        submenu: {
          items: [{ id: 'work', label: 'Work' }],
          search: { placeholder: SEARCH },
        },
      },
    ],
    { x: 12, y: 24 }
  );
  flushSync();
  await tick();

  const row = [...document.querySelectorAll<HTMLElement>('[role=menuitem]')].find((el) =>
    el.textContent?.includes('Tags')
  );
  if (!row) throw new Error('the Tags row did not render');
  row.click();
  flushSync();
  await tick();
}

const originalWidth = window.innerWidth;

afterEach(() => {
  contextMenu.close();
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: originalWidth });
  document.body.innerHTML = '';
});

describe('ContextMenu dismissal', () => {
  it('keeps the sheet and its search field open when the keyboard resizes the height', async () => {
    const { component } = render();
    await openTagsMenuWithSearch();

    const field = document.querySelector<HTMLInputElement>(`input[aria-label="${SEARCH}"]`);
    expect(field).not.toBeNull();
    field!.focus();

    // Soft keyboard: the height shrinks, the width does not.
    window.dispatchEvent(new Event('resize'));
    flushSync();

    expect(contextMenu.isOpen).toBe(true);
    expect(document.body.contains(field)).toBe(true);

    unmount(component);
  });

  it('closes when the window width changes', async () => {
    const { component } = render();
    await openTagsMenuWithSearch();

    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 700 });
    window.dispatchEvent(new Event('resize'));
    flushSync();

    expect(contextMenu.isOpen).toBe(false);

    unmount(component);
  });
});
