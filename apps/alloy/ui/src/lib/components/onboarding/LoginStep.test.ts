import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount } from 'svelte';
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

  it('names the required cookies a paste lacks and keeps the form closed', () => {
    const { onsubmit, submit, type } = render();
    type("curl 'https://www.messenger.com/' -H 'cookie: datr=D1'");

    expect(submit.disabled).toBe(true);
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('c_user, xs');
    expect(onsubmit).not.toHaveBeenCalled();
  });
});

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
