import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

/**
 * The phone tab bar carries the two destinations a phone has (Chats, Settings), hugs its content
 * and sits centred. There is no third surface: a phone swaps the chat list and the conversation
 * inside the Chats tab.
 */

const barSource = readFileSync(resolve(process.cwd(), 'src/lib/components/shell/MobileTabBar.svelte'), 'utf8');

import MobileTabBar from './MobileTabBar.svelte';

function render() {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const onchats = vi.fn();
  const onsettings = vi.fn();
  const component = mount(MobileTabBar, { target, props: { active: 'chats', onchats, onsettings, unread: 3 } });
  flushSync();
  return { target, component, onchats, onsettings };
}

describe('mobile tab bar', () => {
  it('carries two destinations and announces the unread count', () => {
    const { target, component, onchats, onsettings } = render();
    const tabs = [...target.querySelectorAll('button')];
    expect(tabs).toHaveLength(2);
    expect(tabs.map((t) => t.querySelector('.label')?.textContent?.trim())).toEqual(['Chats', 'Settings']);
    expect(tabs[0].getAttribute('aria-label')).toBe('Chats, 3 unread');
    expect(tabs[0].getAttribute('aria-current')).toBe('page');
    expect(tabs[1].getAttribute('aria-current')).toBeNull();
    tabs[1].click();
    expect(onsettings).toHaveBeenCalled();
    tabs[0].click();
    expect(onchats).toHaveBeenCalled();
    unmount(component);
    target.remove();
  });

  it('names the Chats tab without a count when nothing is unread', () => {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const component = mount(MobileTabBar, { target, props: { active: 'settings', onchats: vi.fn(), onsettings: vi.fn() } });
    flushSync();
    const tabs = [...target.querySelectorAll('button')];
    expect(tabs[0].getAttribute('aria-label')).toBe('Chats');
    expect(tabs[1].getAttribute('aria-current')).toBe('page');
    unmount(component);
    target.remove();
  });

  it('caps the announced count at 99+', () => {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const component = mount(MobileTabBar, { target, props: { active: 'chats', onchats: vi.fn(), onsettings: vi.fn(), unread: 250 } });
    flushSync();
    expect(target.querySelector('button')?.getAttribute('aria-label')).toBe('Chats, 99+ unread');
    unmount(component);
    target.remove();
  });

  it('hugs its content instead of stretching the tabs', () => {
    expect(barSource).toContain('width: fit-content;');
    expect(barSource).toContain('margin-inline: auto;');
    expect(barSource).toContain('flex: 0 0 auto;');
    expect(barSource).toContain('min-width: 96px;');
    expect(barSource).not.toContain('flex: 1 1 0;');
  });

  it('acknowledges a press without turning it into the selected state', () => {
    expect(barSource).toContain('.tab:active .glyph {');
    expect(barSource).toContain('.tab:active .label {');
  });
});
