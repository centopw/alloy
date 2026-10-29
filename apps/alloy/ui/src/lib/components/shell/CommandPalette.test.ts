import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { RoomSummary } from '$lib/generated';

const mocks = vi.hoisted(() => ({
  rooms: [] as RoomSummary[],
  onclose: vi.fn(),
  onpick: vi.fn(),
  pushBack: vi.fn(() => () => {}),
  invoke: vi.fn(),
  openProfile: vi.fn(),
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

vi.mock('$lib/ipc', () => ({ client: { invoke: mocks.invoke } }));

vi.mock('$lib/stores/profileCard.svelte', () => ({
  profileCard: { open: mocks.openProfile },
}));

import CommandPalette from './CommandPalette.svelte';

function room(overrides: Partial<RoomSummary> = {}): RoomSummary {
  return {
    id: '!room:example.org',
    name: 'Room',
    membership: 'Joined',
    is_space: false,
    ...overrides,
  } as RoomSummary;
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

beforeEach(() => {
  vi.clearAllMocks();
  mocks.rooms.length = 0;
});

describe('CommandPalette accessibility', () => {
  it('wires the input as a combobox pointing at the first option', () => {
    mocks.rooms.push(room({ id: '!a:example.org', name: 'Alpha' }), room({ id: '!b:example.org', name: 'Beta' }));

    const { target, component } = render();

    const input = target.querySelector('input[role="combobox"]');
    expect(input).not.toBeNull();
    expect(input!.getAttribute('aria-controls')).toBe('palette-results');
    expect(input!.getAttribute('aria-activedescendant')).toBe('palette-option-!a:example.org');

    const options = target.querySelectorAll('[role="option"]');
    expect(options).toHaveLength(2);
    expect(options[0].getAttribute('aria-selected')).toBe('true');
    expect(options[1].getAttribute('aria-selected')).toBe('false');

    unmount(component);
    target.remove();
  });

  it('moves aria-activedescendant as the highlighted row changes', () => {
    mocks.rooms.push(room({ id: '!a:example.org', name: 'Alpha' }), room({ id: '!b:example.org', name: 'Beta' }));

    const { target, component } = render();
    const input = target.querySelector('input[role="combobox"]') as HTMLInputElement;

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    flushSync();
    expect(input.getAttribute('aria-activedescendant')).toBe('palette-option-!b:example.org');

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
    flushSync();
    expect(input.getAttribute('aria-activedescendant')).toBe('palette-option-!a:example.org');

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

  it('picks the highlighted room on Enter', () => {
    mocks.rooms.push(room({ id: '!a:example.org', name: 'Alpha' }), room({ id: '!b:example.org', name: 'Beta' }));

    const { target, component } = render();
    const input = target.querySelector('input[role="combobox"]') as HTMLInputElement;

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    flushSync();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    flushSync();
    expect(mocks.onpick).toHaveBeenCalledWith('!b:example.org');

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

describe('CommandPalette people search', () => {
  it('lists user-directory matches and opens the profile card', async () => {
    vi.useFakeTimers();
    try {
      mocks.invoke.mockResolvedValue({
        type: 'UserSearchResults',
        payload: {
          results: [{ user_id: '@bob:example.org', display_name: 'Bob', avatar_url: null, already_in_room: true }],
          limited: false,
        },
      });
      mocks.rooms.length = 0;
      const { target, component } = render();
      const input = target.querySelector('input') as HTMLInputElement;
      input.value = 'bob';
      input.dispatchEvent(new Event('input', { bubbles: true }));
      await vi.advanceTimersByTimeAsync(300);
      flushSync();

      expect(mocks.invoke).toHaveBeenCalledWith({ action: 'SearchUsers', params: { query: 'bob', limit: 6 } });
      const options = [...target.querySelectorAll('[role="option"]')];
      const person = options.find((o) => o.textContent?.includes('Bob'));
      expect(person, 'the directory match is an option').toBeTruthy();
      expect(person!.textContent).toContain('in a room');

      (person!.querySelector('button') as HTMLButtonElement).click();
      flushSync();
      expect(mocks.openProfile).toHaveBeenCalledWith('@bob:example.org', null, expect.any(Object));
      expect(mocks.onpick).not.toHaveBeenCalled();
      unmount(component);
      target.remove();
    } finally {
      vi.useRealTimers();
    }
  });
});
