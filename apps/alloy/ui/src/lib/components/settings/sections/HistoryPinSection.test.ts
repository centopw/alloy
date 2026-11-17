import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import HistoryPinSection from './HistoryPinSection.svelte';
import { MESSENGER_CAPABILITIES } from '$lib/mock/seed';
import type { HistoryEnrolmentOutcome } from '$lib/generated';

const idle: HistoryEnrolmentOutcome = 'not_supported';

function capabilities(pin_backup: boolean, enrolled: boolean) {
  return { ...MESSENGER_CAPABILITIES, history: { pin_backup, enrolled } };
}

function render(
  history: { pin_backup: boolean; enrolled: boolean },
  outcome: HistoryEnrolmentOutcome = idle
) {
  const onenrol = vi.fn(async () => outcome);
  const target = document.createElement('div');
  document.body.appendChild(target);
  const component = mount(HistoryPinSection, {
    target,
    props: { capabilities: capabilities(history.pin_backup, history.enrolled), busy: false, onenrol },
  });
  flushSync();
  return { target, component, onenrol };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

afterEach(() => {
  document.body.replaceChildren();
  vi.clearAllMocks();
});

describe('HistoryPinSection', () => {
  it('offers no PIN when the account has no encrypted backup', () => {
    const { target, component } = render({ pin_backup: false, enrolled: false });
    expect(target.querySelector('input')).toBeNull();
    expect(target.textContent?.trim()).toBe('');
    unmount(component);
  });

  it('offers the six-digit PIN only while a backup exists and the device is not enrolled', () => {
    const { target, component } = render({ pin_backup: true, enrolled: false });
    const input = target.querySelector<HTMLInputElement>('input');
    expect(input).not.toBeNull();
    expect(input!.maxLength).toBe(6);
    expect(input!.inputMode).toBe('numeric');
    expect(target.querySelector('button[type="submit"]')?.hasAttribute('disabled')).toBe(true);
    unmount(component);
  });

  it('submits the PIN once and confirms history is being read', async () => {
    const { target, component, onenrol } = render({ pin_backup: true, enrolled: false }, 'enrolled');
    const input = target.querySelector<HTMLInputElement>('input')!;
    input.value = '12ab3456';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    expect(input.value).toBe('123456');

    target.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await settle();

    expect(onenrol).toHaveBeenCalledTimes(1);
    expect(onenrol).toHaveBeenCalledWith('123456');
    expect(target.textContent).toContain('History is being read');
    expect(target.querySelector('input')).toBeNull();
    unmount(component);
  });

  it('shows how many PIN attempts are left after a rejection', async () => {
    const { target, component } = render(
      { pin_backup: true, enrolled: false },
      { pin_rejected: { attempts_remaining: 2 } }
    );
    const input = target.querySelector<HTMLInputElement>('input')!;
    input.value = '000000';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    target.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await settle();

    expect(target.textContent).toContain('2 attempts left');
    unmount(component);
  });

  it('names a locked archive and an account without a backup, each in its own words', async () => {
    const locked = render({ pin_backup: true, enrolled: false }, 'locked_out');
    const input = locked.target.querySelector<HTMLInputElement>('input')!;
    input.value = '000000';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    locked.target
      .querySelector('form')!
      .dispatchEvent(new Event('submit', { cancelable: true }));
    await settle();
    expect(locked.target.textContent).toContain('locked');
    unmount(locked.component);
    locked.target.remove();

    const missing = render({ pin_backup: true, enrolled: false }, 'pin_backup_unavailable');
    const field = missing.target.querySelector<HTMLInputElement>('input')!;
    field.value = '000000';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    missing.target
      .querySelector('form')!
      .dispatchEvent(new Event('submit', { cancelable: true }));
    await settle();
    expect(missing.target.textContent).toContain('no encrypted backup');
    unmount(missing.component);
  });
});
