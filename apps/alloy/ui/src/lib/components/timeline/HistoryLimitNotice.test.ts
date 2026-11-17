import { afterEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import HistoryLimitNotice from './HistoryLimitNotice.svelte';
import type { HistoryLimit } from '$lib/generated';

function render(limit: HistoryLimit | null) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const component = mount(HistoryLimitNotice, { target, props: { limit } });
  flushSync();
  return { target, component, text: document.body.textContent ?? '' };
}

afterEach(() => {
  document.body.replaceChildren();
});

describe('HistoryLimitNotice', () => {
  it('says why older history is unavailable in one line', () => {
    const { target, text, component } = render('not_enrolled');
    expect(text).toContain('encrypted backup');
    expect(target.querySelectorAll('p')).toHaveLength(1);
    expect(target.querySelector('p')?.getAttribute('role')).toBe('note');
    unmount(component);
  });

  it('renders nothing for the start of what exists', () => {
    const { target, component } = render('start');
    expect(target.textContent).toBe('');
    unmount(component);
  });

  it('names the locked archive and the missing chat, each in its own words', () => {
    const locked = render('locked_out');
    expect(locked.text).toContain('locked');
    unmount(locked.component);
    locked.target.remove();

    const missing = render('not_in_archive');
    expect(missing.text).toContain('not in the encrypted backup');
    unmount(missing.component);
  });
});
