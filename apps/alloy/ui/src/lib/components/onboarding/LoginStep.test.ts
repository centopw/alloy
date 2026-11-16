import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick } from 'svelte';
import LoginStep from './LoginStep.svelte';
import type { LoginInputDto, LoginStepDto } from '$lib/generated/index.js';

const step: LoginStepDto = {
  type: 'cookies',
  fields: [
    { name: 'datr', label: 'datr cookie', required: true },
    { name: 'c_user', label: 'c_user cookie', required: true },
    { name: 'sb', label: 'sb cookie', required: false },
    { name: 'xs', label: 'xs cookie', required: true }
  ],
  help_url: null
};

function render(shown: LoginStepDto = step) {
  const onsubmit = vi.fn<(input: LoginInputDto) => void>();
  mount(LoginStep, {
    target: document.body,
    props: {
      step: shown,
      networkName: 'Messenger',
      busy: false,
      error: null,
      onsubmit,
      onswitch: () => {},
      oncancel: () => {},
      onretry: () => {},
      onback: () => {}
    }
  });
  flushSync();
  const paste = document.querySelector<HTMLTextAreaElement | HTMLInputElement>('textarea, input')!;
  const submit = document.querySelector<HTMLButtonElement>('button[type="submit"]')!;
  const type = (text: string) => {
    paste.value = text;
    paste.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
  };
  return { onsubmit, submit, type };
}

afterEach(() => {
  document.body.replaceChildren();
});

describe('LoginStep browser sign-in', () => {
  it('submits the cookies read from a request pasted as cURL', () => {
    const { onsubmit, submit, type } = render();
    expect(submit.disabled).toBe(true);

    type("curl 'https://www.messenger.com/' -H 'cookie: datr=D1; c_user=42; xs=X%3A1; other=z'");
    expect(submit.disabled).toBe(false);
    document.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));

    expect(onsubmit).toHaveBeenCalledWith({
      type: 'cookies',
      values: { datr: 'D1', c_user: '42', sb: '', xs: 'X%3A1' }
    });
  });

  it('renders a checkpoint challenge with the site action and no code field', () => {
    render({ type: 'challenge', kind: 'checkpoint', message: 'confirm this sign-in' });
    const text = document.body.textContent ?? '';
    expect(text).toContain('confirm this sign-in');
    const labels = [...document.querySelectorAll('button')].map((b) => b.textContent?.trim());
    expect(labels).toContain('Open Facebook');
    // A checkpoint is cleared on the network's site; a code field there would be dead.
    expect(document.querySelector('input[autocomplete="one-time-code"]')).toBeNull();
  });

  it('renders a two-factor challenge with the code field the attempt is waiting for', () => {
    const { onsubmit } = render({ type: 'challenge', kind: 'two_factor', message: 'enter the code' });
    const input = document.querySelector<HTMLInputElement>('input[autocomplete="one-time-code"]');
    expect(input).not.toBeNull();
    input!.value = '123456';
    input!.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    document.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    // The attempt stays open across a challenge, so the code is a plain `code` input.
    expect(onsubmit).toHaveBeenCalledWith({ type: 'code', code: '123456' });
  });

  it('points an unsupported-MFA challenge at the security settings and keeps the step open', () => {
    const opened = vi.spyOn(window, 'open').mockImplementation(() => null);
    render({ type: 'challenge', kind: 'unsupported_mfa', message: '' });
    const text = document.body.textContent ?? '';
    expect(text).toContain('This sign-in needs a method this app cannot use');

    const buttons = [...document.querySelectorAll('button')];
    const settings = buttons.find((b) => b.textContent?.trim() === 'Open Facebook')!;
    settings.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(opened).toHaveBeenCalledWith(
      'https://www.facebook.com/settings?tab=security',
      '_blank',
      'noopener,noreferrer'
    );

    // No local answer clears it, so the step stays open on its own alternatives.
    expect(buttons.map((b) => b.textContent?.trim())).toContain('Sign in with your browser instead');
    expect(document.querySelector('input[autocomplete="one-time-code"]')).toBeNull();
    opened.mockRestore();
  });

  it('names the required cookies a paste lacks and keeps the form closed', () => {
    const { onsubmit, submit, type } = render();
    type("curl 'https://www.messenger.com/' -H 'cookie: datr=D1'");

    expect(submit.disabled).toBe(true);
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('c_user, xs');
    expect(onsubmit).not.toHaveBeenCalled();
  });

  it('fills the cookies from the clipboard in one click', async () => {
    stubClipboard(() =>
      Promise.resolve("curl 'https://www.messenger.com/' -H 'cookie: datr=D1; c_user=42; xs=X%3A1'")
    );
    const { submit } = render();
    expect(submit.disabled).toBe(true);

    click('Paste from clipboard');
    await tick();
    flushSync();

    expect(submit.disabled).toBe(false);
    expect(document.body.textContent).toContain('Found the cookies Alloy needs.');
    expect(document.body.textContent).not.toContain('would not hand over');
  });

  it('says so when the browser refuses the clipboard or holds nothing usable', async () => {
    stubClipboard(() => Promise.reject(new Error('denied')));
    const { submit } = render();

    click('Paste from clipboard');
    await tick();
    flushSync();

    expect(document.body.textContent).toContain('would not hand over the clipboard');
    expect(submit.disabled).toBe(true);

    stubClipboard(() => Promise.resolve('   '));
    click('Paste from clipboard');
    await tick();
    flushSync();
    expect(submit.disabled).toBe(true);
  });
});

/** Stands in for the OS clipboard; jsdom has no `navigator.clipboard`. */
function stubClipboard(readText: () => Promise<string>) {
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { readText } });
}

function click(label: string) {
  const button = [...document.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label
  );
  expect(button, label).toBeTruthy();
  button!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
}

describe('LoginStep phone number', () => {
  it('submits international format however the number was typed, and follows a typed country', () => {
    const { onsubmit, submit, type } = render({ type: 'phone_number' });
    expect(submit.disabled).toBe(true);

    type('+84 0367337123');
    expect(document.querySelector('.select-trigger')?.textContent).toContain('+84');
    document.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));

    expect(onsubmit).toHaveBeenCalledWith({ type: 'phone_number', phone: '+84367337123' });
  });
});
