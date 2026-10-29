import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

/**
 * T34: the phone tab bar spread three tabs across the screen. It now carries the two destinations
 * that exist (Chats, Settings), hugs its content and sits centred; public rooms are reached from
 * the room-list menu on every layout instead of from a tab.
 */

const barSource = readFileSync(resolve(process.cwd(), 'src/lib/components/shell/MobileTabBar.svelte'), 'utf8');
const sidebar = readFileSync(resolve(process.cwd(), 'src/lib/components/shell/Sidebar.svelte'), 'utf8');

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
    expect(barSource).not.toContain('Discover');
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

  it('offers public rooms from the room-list menu on every layout', () => {
    const create = sidebar.slice(sidebar.indexOf('function createItems()'), sidebar.indexOf('/** Pane header'));
    expect(create).toContain("label: 'Browse public rooms...'");
    expect(create).not.toContain('isPhone');
  });
});
