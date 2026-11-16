import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount, type Component } from 'svelte';
import DeviceList from './DeviceList.svelte';
import DevicesSync from './DevicesSync.svelte';
import PairingAccept from './PairingAccept.svelte';
import { isCompletePairingCode, normalizePairingCode } from './pairing';
import type { DeviceView, PairingState, PairingView } from './types';
import SettingsDialog from '../SettingsDialog.svelte';

import { t } from '$lib/i18n';

const teardown: Array<() => void> = [];

function render<Props extends Record<string, unknown>>(
  component: Component<Props>,
  props: Props
): HTMLElement {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(component, { target, props });
  teardown.push(() => unmount(instance));
  flushSync();
  return target;
}

afterEach(() => {
  for (const destroy of teardown.splice(0)) {
    destroy();
  }
  document.body.replaceChildren();
});

function pairing(state: PairingState): PairingView {
  return {
    state,
    ticket: 'alloy://pair?p=test-ticket',
    code: 'AB34CD56',
    sas: '123456',
    peerLabel: 'Pixel',
    expiresAtMs: Date.now() + 60_000,
    error: state === 'failed' ? 'The peer vanished.' : null,
  };
}

const STATE_TEXT: Record<PairingState, string> = {
  waiting: t('sync-pairing-state-waiting'),
  peer_found: t('sync-pairing-state-peer-found'),
  sas_ready: t('sync-pairing-state-sas-ready'),
  transferring: t('sync-pairing-state-transferring'),
  done: t('sync-pairing-state-done'),
  declined: t('sync-pairing-state-declined'),
  expired: t('sync-pairing-state-expired'),
  failed: t('sync-pairing-state-failed'),
};

const STATES = Object.keys(STATE_TEXT) as PairingState[];

describe('settings navigation', () => {
  it('reaches Devices & Sync from the settings dialog', () => {
    const target = render(SettingsDialog, { open: true, onclose: () => {} });

    const entry = [...target.querySelectorAll('button')].find(
      (button) => button.textContent?.trim() === t('settings-devices-sync')
    );
    expect(entry, 'the nav lists Devices & Sync').toBeTruthy();

    entry!.click();
    flushSync();

    expect(target.textContent).toContain(t('sync-this-device'));
    expect(target.textContent).toContain(t('sync-pair-a-device'));
    expect(target.textContent).toContain(t('sync-server'));
    expect(target.textContent).toContain(t('sync-devices'));
    expect(target.textContent).toContain(t('sync-recovery'));
  });
});

describe('DevicesSync pairing state machine', () => {
  it.each(STATES)('renders the %s state with its own status line', (state) => {
    const target = render(DevicesSync, { pairing: pairing(state), role: 'offer' as const });

    const status = target.querySelector('[data-pairing-state]');
    expect(status, 'a status line is rendered').toBeTruthy();
    expect(status!.getAttribute('data-pairing-state')).toBe(state);
    expect(status!.textContent?.trim()).toBe(STATE_TEXT[state]);

    // The fixed-height body is present in every state, which is what keeps the panels below still.
    expect(target.querySelector('.pair-body')).toBeTruthy();
  });

  it('shows the QR fallback code while waiting as the offering side', () => {
    const target = render(DevicesSync, { pairing: pairing('waiting'), role: 'offer' as const });
    expect(target.querySelector('output')?.textContent?.trim()).toBe('AB34CD56');
    expect(target.textContent).toContain(t('sync-pair-crosses'));
  });

  it('shows the six digits and both SAS answers once the SAS is ready', () => {
    const target = render(DevicesSync, { pairing: pairing('sas_ready'), role: 'accept' as const });

    expect(target.textContent).toContain(t('sync-sas-warning'));
    const digits = target.querySelector('[aria-label="' + t('sync-sas-label') + '"]');
    expect(digits?.textContent?.replace(/\s+/g, '')).toBe('123456');
    const buttons = [...target.querySelectorAll('button')].map((button) =>
      button.textContent?.trim()
    );
    expect(buttons).toContain(t('sync-sas-match'));
    expect(buttons).toContain(t('sync-sas-no-match'));
  });
});

describe('PairingAccept code field', () => {
  it('normalises spaces, dashes and lower case as the code is typed', () => {
    const target = render(PairingAccept, { onsubmit: () => {} });
    const input = target.querySelector('input') as HTMLInputElement | null;
    expect(input, 'the code field is an input').toBeTruthy();

    input!.value = 'ab 34-cd56';
    input!.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();

    expect(input!.value).toBe('AB34CD56');
  });

  it('submits the pasted ticket with the normalised code', () => {
    const onsubmit = vi.fn();
    const target = render(PairingAccept, { onsubmit });

    const ticket = target.querySelector('textarea') as HTMLTextAreaElement | null;
    const code = target.querySelector('input') as HTMLInputElement | null;
    expect(ticket && code).toBeTruthy();

    const setValue = (element: HTMLTextAreaElement | HTMLInputElement, value: string) => {
      element.value = value;
      element.dispatchEvent(new Event('input', { bubbles: true }));
    };
    setValue(ticket!, '  alloy://pair?p=test-ticket  ');
    setValue(code!, 'ab-34 cd-56');
    flushSync();

    const submit = [...target.querySelectorAll('button')].find(
      (button) => button.getAttribute('type') === 'submit'
    );
    expect(submit?.textContent?.trim()).toBe(t('sync-accept-submit'));
    submit!.click();
    flushSync();

    expect(onsubmit).toHaveBeenCalledWith('alloy://pair?p=test-ticket', 'AB34CD56');
  });

  it('rejects codes that are short or outside the pairing alphabet', () => {
    expect(isCompletePairingCode('AB12CD3')).toBe(false);
    expect(isCompletePairingCode('AB12CD0I')).toBe(false);
    expect(isCompletePairingCode('ab-34 cd-56')).toBe(true);
    expect(normalizePairingCode('abcd-efgh-ijkl')).toBe('ABCDEFGH');
  });
});

describe('DeviceList revoke', () => {
  const device: DeviceView = {
    id: 'device-2',
    label: 'Phone',
    enrolledAtMs: Date.UTC(2026, 10, 1),
    lastSyncMs: Date.UTC(2026, 10, 15),
    epoch: 4,
  };

  it('does not fire the callback until the confirmation is accepted', () => {
    const onrevoke = vi.fn();
    const target = render(DeviceList, { devices: [device], onrevoke });

    const revoke = [...target.querySelectorAll('button')].find(
      (button) => button.textContent?.trim() === t('sync-devices-revoke')
    );
    expect(revoke).toBeTruthy();
    revoke!.click();
    flushSync();

    expect(onrevoke, 'opening the confirmation revokes nothing').not.toHaveBeenCalled();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog, 'the confirmation is a dialog').toBeTruthy();
    expect(dialog!.textContent).toContain('Phone');

    const confirm = [...dialog!.querySelectorAll('button')].find(
      (button) => button.textContent?.trim() === t('sync-devices-revoke-confirm')
    );
    expect(confirm).toBeTruthy();
    confirm!.click();
    flushSync();

    expect(onrevoke).toHaveBeenCalledTimes(1);
    expect(onrevoke).toHaveBeenCalledWith('device-2');
  });
});
