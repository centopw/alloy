import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick } from 'svelte';
import LoginStep from './LoginStep.svelte';
import type { LoginInputDto, LoginStepDto } from '$lib/generated/index.js';

const chrome = vi.hoisted(() => ({ supported: false, import: vi.fn() }));

vi.mock('$lib/platform/chromeSession', () => ({
  isChromeSessionSupported: () => chrome.supported,
  importChromeSession: (...args: unknown[]) => chrome.import(...args)
}));

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
  chrome.supported = false;
  chrome.import.mockReset();
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

function chromeButton(): HTMLButtonElement | undefined {
  return [...document.querySelectorAll('button')].find((b) =>
    b.textContent?.includes('Use the Chrome session')
  );
}

describe('LoginStep Chrome session import', () => {
  const imported = {
    cookies: { datr: 'D1', c_user: '42', xs: 'X%3A1', sb: 'SB1', fr: 'FR1' },
    profile: 'Default',
    active: true
  };

  it('offers the action without reading Chrome, and reads it once pressed', async () => {
    chrome.supported = true;
    chrome.import.mockResolvedValue(imported);
    const { onsubmit, submit } = render();

    const button = chromeButton();
    expect(button).toBeTruthy();
    // The lazy rule: mounting the step reaches no backend by itself.
    expect(chrome.import).not.toHaveBeenCalled();
    expect(submit.disabled).toBe(true);

    button!.click();
    await vi.waitFor(() => expect(chrome.import).toHaveBeenCalledTimes(1));
    await tick();
    flushSync();

    expect(document.body.textContent).toContain('Session imported from Chrome.');
    expect(document.body.textContent).toContain('Sign out of Messenger in Chrome after this');
    expect(submit.disabled).toBe(false);

    document.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    expect(onsubmit).toHaveBeenCalledWith({
      type: 'cookies',
      values: { datr: 'D1', c_user: '42', sb: 'SB1', xs: 'X%3A1' }
    });
  });

  it('surfaces one readable failure and leaves the manual form as it was', async () => {
    chrome.supported = true;
    chrome.import.mockRejectedValue('This Chrome profile is not signed in to Messenger.');
    const { submit } = render();

    chromeButton()!.click();
    await vi.waitFor(() =>
      expect(document.querySelector('[role="alert"]')?.textContent).toContain(
        'not signed in to Messenger'
      )
    );
    expect(submit.disabled).toBe(true);
    // No automatic fallback: the same manual steps are still the only other way in.
    expect(document.body.textContent).toContain('Request copied as cURL');
  });

  it('shows nothing about Chrome where there is none', () => {
    chrome.supported = false;
    render();
    expect(chromeButton()).toBeUndefined();
    expect(document.body.textContent).not.toContain('Chrome');
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

/** Extracts the body of a top-level brace-delimited block starting at `headerIndex`. */
function extractBlock(source: string, headerIndex: number): string {
  const open = source.indexOf('{', headerIndex);
  expect(open).toBeGreaterThanOrEqual(0);
  let depth = 0;
  for (let i = open; i < source.length; i++) {
    if (source[i] === '{') depth++;
    else if (source[i] === '}') {
      depth--;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  throw new Error('Unclosed CSS block');
}

describe('Onboarding mobile scroll and Account mount regression', () => {
  it('allows vertical scrolling on mobile in Onboarding.svelte without Grid + overflow: hidden deadlock', () => {
    const onboarding = readFileSync(resolve(__dirname, 'Onboarding.svelte'), 'utf8');
    const styleStart = onboarding.indexOf('<style>');
    expect(styleStart).toBeGreaterThanOrEqual(0);
    const style = onboarding.slice(styleStart);

    const onboardingRule = extractBlock(style, style.indexOf('.onboarding {'));
    expect(onboardingRule).toMatch(/display:\s*flex;/);
    expect(onboardingRule).toMatch(/flex-direction:\s*column;/);
    expect(onboardingRule).toMatch(/overflow-y:\s*auto;/);
    expect(onboardingRule).toMatch(/overscroll-behavior:\s*contain;/);
    expect(onboardingRule).toMatch(/touch-action:\s*pan-y;/);

    const panelRule = extractBlock(style, style.indexOf('.panel {'));
    expect(panelRule).toMatch(/margin:\s*auto;/);
    expect(panelRule).toMatch(/flex-shrink:\s*0;/);

    const mobileQueryIdx = style.indexOf('@media (max-width: 899px) {');
    expect(mobileQueryIdx).toBeGreaterThanOrEqual(0);
    const mobileBlock = extractBlock(style, mobileQueryIdx);

    const mobileOnboarding = extractBlock(mobileBlock, mobileBlock.indexOf('.onboarding {'));
    expect(mobileOnboarding).toMatch(/padding:\s*0;/);

    const mobilePanel = extractBlock(mobileBlock, mobileBlock.indexOf('.panel,'));
    expect(mobilePanel).toMatch(/overflow:\s*visible;/);
    expect(mobilePanel).toMatch(/min-height:\s*100%;/);
    expect(mobilePanel).toMatch(/height:\s*auto;/);
    expect(mobilePanel).toMatch(/flex:\s*1\s+0\s+auto;/);
    expect(mobilePanel).toMatch(/margin:\s*0;/);

    const mobileClose = extractBlock(mobileBlock, mobileBlock.indexOf('.panel-close {'));
    expect(mobileClose).toMatch(/position:\s*sticky;/);
    expect(mobileBlock).toMatch(/var\(--alloy-safe-area-bottom\)/);
  });

  it('guards hover and adds touch target heights and bottom clearance in LoginStep.svelte', () => {
    const loginStep = readFileSync(resolve(__dirname, 'LoginStep.svelte'), 'utf8');
    const style = loginStep.slice(loginStep.indexOf('<style>'));

    expect(style).toContain('@media (hover: hover)');
    expect(style).toMatch(/@media \(max-width:\s*899px\),\s*\(pointer:\s*coarse\)/);
    expect(style).toMatch(/min-height:\s*var\(--alloy-touch-target,\s*44px\)/);
    expect(style).toMatch(/var\(--alloy-safe-area-bottom\)/);
  });

  it('does not import or mount <Onboarding inside Account.svelte', () => {
    const account = readFileSync(resolve(__dirname, '../settings/sections/Account.svelte'), 'utf8');
    expect(account).not.toContain('<Onboarding');
    expect(account).not.toContain('Onboarding.svelte');
    expect(account).toContain('session.openAddAccount()');
  });
});
