import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { RoomSummary } from '$lib/generated';

const mocks = vi.hoisted(() => ({
  rooms: [] as RoomSummary[],
  onclose: vi.fn(),
  onpick: vi.fn(),
  pushBack: vi.fn(() => () => {}),
}));

vi.mock('$lib/stores/rooms.svelte', () => ({
  roomsStore: { rooms: mocks.rooms },
  roomTitle: (r: RoomSummary) => r.name ?? r.id,
}));

vi.mock('$lib/stores/viewport.svelte', () => ({
  viewport: { isPhone: false },
}));

vi.mock('$lib/platform/backstack', () => ({
  pushBack: mocks.pushBack,
}));

import CommandPalette from './CommandPalette.svelte';

function room(overrides: Partial<RoomSummary> = {}): RoomSummary {
  return {
    id: 'chat-a',
    account_id: 'account-1',
    network: 'telegram',
    name: 'Chat',
    avatar: null,
    kind: 'direct',
    is_direct: true,
    unread_notifications: { highlight_count: 0, notification_count: 0 },
    latest_event: null,
    pinned: false,
    archived: false,
    muted_until: null,
    tags: [],
    ...overrides,
  };
}

function render(open = true) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const component = mount(CommandPalette, {
    target,
    props: { open, onclose: mocks.onclose, onpick: mocks.onpick },
  });
  flushSync();
  return { target, component };
}

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event('input', { bubbles: true }));
  flushSync();
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.rooms.length = 0;
});

describe('CommandPalette accessibility', () => {
  it('wires the input as a combobox pointing at the first option', () => {
    mocks.rooms.push(room({ id: 'chat-a', name: 'Alpha' }), room({ id: 'chat-b', name: 'Beta' }));

    const { target, component } = render();

    const input = target.querySelector('input[role="combobox"]');
    expect(input).not.toBeNull();
    expect(input!.getAttribute('aria-controls')).toBe('palette-results');
    expect(input!.getAttribute('aria-activedescendant')).toBe('palette-option-chat-a');

    const options = target.querySelectorAll('[role="option"]');
    expect(options).toHaveLength(2);
    expect(options[0].getAttribute('aria-selected')).toBe('true');
    expect(options[1].getAttribute('aria-selected')).toBe('false');

    unmount(component);
    target.remove();
  });

  it('moves aria-activedescendant as the highlighted row changes', () => {
    mocks.rooms.push(room({ id: 'chat-a', name: 'Alpha' }), room({ id: 'chat-b', name: 'Beta' }));

    const { target, component } = render();
    const input = target.querySelector('input[role="combobox"]') as HTMLInputElement;

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    flushSync();
    expect(input.getAttribute('aria-activedescendant')).toBe('palette-option-chat-b');

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
    flushSync();
    expect(input.getAttribute('aria-activedescendant')).toBe('palette-option-chat-a');

    unmount(component);
    target.remove();
  });

  it('closes on Escape', () => {
    mocks.rooms.push(room());

    const { target, component } = render();
    const input = target.querySelector('input[role="combobox"]') as HTMLInputElement;

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    flushSync();
    expect(mocks.onclose).toHaveBeenCalledTimes(1);

    unmount(component);
    target.remove();
  });

  it('picks the highlighted chat on Enter', () => {
    mocks.rooms.push(room({ id: 'chat-a', name: 'Alpha' }), room({ id: 'chat-b', name: 'Beta' }));

    const { target, component } = render();
    const input = target.querySelector('input[role="combobox"]') as HTMLInputElement;

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    flushSync();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    flushSync();
    expect(mocks.onpick).toHaveBeenCalledWith('chat-b');

    unmount(component);
    target.remove();
  });

  it('leaves aria-activedescendant unset when there are no results', () => {
    const { target, component } = render();
    const input = target.querySelector('input[role="combobox"]');
    expect(input!.getAttribute('aria-activedescendant')).toBeNull();

    unmount(component);
    target.remove();
  });
});

describe('CommandPalette search', () => {
  it('narrows the list to chats whose title matches, ignoring case', () => {
    mocks.rooms.push(
      room({ id: 'chat-a', name: 'Alpha team' }),
      room({ id: 'chat-b', name: 'Beta crew' }),
      room({ id: 'chat-c', name: 'Gamma' }),
    );

    const { target, component } = render();
    const input = target.querySelector('input[role="combobox"]') as HTMLInputElement;
    type(input, 'CREW');

    const options = [...target.querySelectorAll('[role="option"]')];
    expect(options).toHaveLength(1);
    expect(options[0].textContent).toContain('Beta crew');
    expect(input.getAttribute('aria-activedescendant')).toBe('palette-option-chat-b');

    unmount(component);
    target.remove();
  });

  it('offers a chat with no stored title by its id', () => {
    mocks.rooms.push(room({ id: 'telegram-77', name: null }));

    const { target, component } = render();
    expect(target.querySelector('[role="option"]')?.textContent).toContain('telegram-77');

    unmount(component);
    target.remove();
  });
});
