import { describe, expect, it } from 'vitest';
import {
  isSeparator,
  isSubmenuAction,
  resolveSubmenuItems,
  submenuView,
  type MenuAction,
  type MenuItem,
  type MenuSubmenu,
} from './contextmenu.svelte';

const SEP: MenuItem = { separator: true };
const apple: MenuAction = { id: 'apple', label: 'Apple' };
const banana: MenuAction = { id: 'banana', label: 'Banana' };
const cherry: MenuAction = { id: 'cherry', label: 'Cherry' };
const ITEMS: MenuItem[] = [apple, SEP, banana, cherry];

describe('submenuView', () => {
  it('returns every item untouched for an empty query and offers no create row', () => {
    const view = submenuView(ITEMS, '   ', true);
    expect(view.items).toBe(ITEMS);
    expect(view.query).toBe('');
    expect(view.showCreate).toBe(false);
  });

  it('filters by case-insensitive label substring and drops separators', () => {
    const view = submenuView(ITEMS, 'AN', false);
    expect(view.items.map((i) => (isSeparator(i) ? '-' : i.id))).toEqual(['banana']);
    const fruit = submenuView(ITEMS, 'a', false);
    expect(fruit.items.map((i) => (isSeparator(i) ? '-' : i.id))).toEqual(['apple', 'banana']);
  });

  it('trims the query before filtering and reports the trimmed query', () => {
    const view = submenuView(ITEMS, '  cher ', false);
    expect(view.query).toBe('cher');
    expect(view.items.map((i) => (isSeparator(i) ? '-' : i.id))).toEqual(['cherry']);
  });

  it('offers the create row for a non-empty query with no exact label match', () => {
    const view = submenuView(ITEMS, 'dates', true);
    expect(view.items).toEqual([]);
    expect(view.showCreate).toBe(true);
  });

  it('suppresses the create row when a label equals the query, ignoring case and spacing', () => {
    const view = submenuView(ITEMS, ' banana ', true);
    expect(view.showCreate).toBe(false);
  });

  it('suppresses the create row when the caller does not allow creation', () => {
    const view = submenuView(ITEMS, 'dates', false);
    expect(view.showCreate).toBe(false);
  });
});

describe('isSubmenuAction', () => {
  it('is true only for actions that carry a submenu', () => {
    const submenu: MenuSubmenu = { items: [apple] };
    expect(isSubmenuAction({ id: 'more', label: 'More', submenu })).toBe(true);
    expect(isSubmenuAction(apple)).toBe(false);
    expect(isSubmenuAction(SEP)).toBe(false);
  });
});

describe('resolveSubmenuItems', () => {
  it('returns a static item list as given', () => {
    const submenu: MenuSubmenu = { items: ITEMS };
    expect(resolveSubmenuItems(submenu)).toBe(ITEMS);
  });

  it('calls a factory each time so items reflect current state', () => {
    let calls = 0;
    const submenu: MenuSubmenu = {
      items: () => {
        calls += 1;
        return [{ id: `call-${calls}`, label: `Call ${calls}` }];
      },
    };
    expect(resolveSubmenuItems(submenu)).toEqual([{ id: 'call-1', label: 'Call 1' }]);
    expect(resolveSubmenuItems(submenu)).toEqual([{ id: 'call-2', label: 'Call 2' }]);
  });
});
