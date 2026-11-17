import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import Onboarding from './Onboarding.svelte';
import { session } from '$lib/stores/session.svelte';

import { t } from '$lib/i18n';

const teardown: Array<() => void> = [];

function render(): HTMLElement {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(Onboarding, { target });
  teardown.push(() => unmount(instance));
  flushSync();
  return target;
}

/** The network list and the two extra ways in arrive from the engine on mount. */
async function settle() {
  for (let i = 0; i < 12; i += 1) {
    await Promise.resolve();
    flushSync();
  }
}

function byText(target: HTMLElement, text: string): HTMLButtonElement {
  const found = [...target.querySelectorAll('button')].find((button) =>
    button.textContent?.includes(text)
  );
  if (!found) throw new Error(`no button containing "${text}"`);
  return found;
}

afterEach(() => {
  vi.useRealTimers();
  for (const destroy of teardown.splice(0)) destroy();
  document.body.replaceChildren();
});

describe('Onboarding: choosing a way in', () => {
  it('offers a network sign-in, a backup restore and linking to an existing Alloy', async () => {
    const target = render();
    await settle();

    const text = target.textContent ?? '';
    expect(text).toContain(t('network-telegram'));
    expect(text).toContain(t('onboarding-restore-title'));
    expect(text).toContain(t('onboarding-link-title'));
  });

  it('opens the link path with the code entry focused', async () => {
    const target = render();
    await settle();

    byText(target, t('onboarding-link-title')).click();
    await settle();

    const code = target.querySelector('input') as HTMLInputElement | null;
    expect(code, 'the code field is rendered').toBeTruthy();
    expect(document.activeElement).toBe(code);
  });

  it('says why a malformed code was refused and keeps the other ways in reachable', async () => {
    const target = render();
    await settle();

    byText(target, t('onboarding-link-title')).click();
    await settle();

    // A ticket with a code that is not eight characters of the alphabet: the form names the code.
    const ticket = target.querySelector('textarea') as HTMLTextAreaElement;
    const code = target.querySelector('input') as HTMLInputElement;
    ticket.value = 'alloy://pair?p=test-ticket';
    ticket.dispatchEvent(new Event('input', { bubbles: true }));
    code.value = 'AB3';
    code.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    byText(target, t('sync-accept-submit')).click();
    await settle();
    expect(target.textContent).toContain(t('sync-accept-invalid-code'));

    code.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();

    expect(target.textContent).toContain(t('onboarding-start-title'));
    expect(byText(target, t('network-telegram'))).toBeTruthy();
  });
});

describe('Onboarding: linking after a successful accept', () => {
  it('confirms the SAS and leaves onboarding, never returning to the start screen', async () => {
    const target = render();
    await settle();

    byText(target, t('onboarding-link-title')).click();
    await settle();

    // The mock handshake advances on PairingStatus polls; timers drive them without waiting.
    vi.useFakeTimers();
    const ticket = target.querySelector('textarea') as HTMLTextAreaElement;
    const code = target.querySelector('input') as HTMLInputElement;
    ticket.value = 'alloy://pair?p=test-ticket';
    ticket.dispatchEvent(new Event('input', { bubbles: true }));
    code.value = 'AB34CD56';
    code.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    byText(target, t('sync-accept-submit')).click();
    await vi.advanceTimersByTimeAsync(4000);
    flushSync();

    expect(target.textContent).toContain(t('sync-sas-title'));
    byText(target, t('sync-sas-match')).click();
    vi.useRealTimers();
    await settle();

    // The engine's account list is what decides the screen: a stored account is the app.
    expect(session.phase).toBe('ready');
    expect(target.textContent ?? '').not.toContain(t('onboarding-start-title'));
  });
});
