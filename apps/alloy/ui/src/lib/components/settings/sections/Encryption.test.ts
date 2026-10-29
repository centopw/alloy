import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { SessionInfoDto } from '$lib/generated';

/**
 * T24: the Encryption panel offered "Start verification" unconditionally, so a device that was
 * already verified was still told to verify — the same screen that is supposed to confirm D4 = C
 * worked. Devices.svelte already gated the control on the session's verified flag; this panel now
 * shows the state and only offers verification when it is false.
 */

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  requestVerification: vi.fn(),
  refreshRecovery: vi.fn(),
  onclose: vi.fn(),
}));

vi.mock('$lib/ipc', () => ({ client: { invoke: mocks.invoke } }));

vi.mock('$lib/stores/verification.svelte', () => ({
  verification: {
    view: { step: 'idle' },
    recovery: 'Unknown',
    recoveryKey: null,
    refreshRecovery: mocks.refreshRecovery,
    requestVerification: mocks.requestVerification,
  },
}));

import Encryption from './Encryption.svelte';

function session(verified: boolean): SessionInfoDto {
  return {
    device_id: 'DEVICEUI',
    ed25519_key: 'AAAA BBBB CCCC DDDD',
    curve25519_key: null,
    cross_signing_ready: true,
    backup_enabled: true,
    verified,
  } as SessionInfoDto;
}

async function render(verified: boolean) {
  mocks.invoke.mockResolvedValue({ type: 'SessionInfo', payload: { info: session(verified) } });
  const target = document.createElement('div');
  document.body.appendChild(target);
  const component = mount(Encryption, { target, props: { onclose: mocks.onclose } });
  flushSync();
  await vi.waitFor(() => expect(target.textContent).toContain('Status'));
  flushSync();
  return { target, component };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('Encryption panel device status', () => {
  it('shows a verified session as verified and offers no verification', async () => {
    const { target, component } = await render(true);
    expect(target.textContent).toContain('Verified');
    expect(target.textContent).not.toContain('Not verified');
    expect(target.textContent).not.toContain('Start verification');
    expect(target.textContent).not.toContain('Verify this device');
    unmount(component);
    target.remove();
  });

  it('shows an unverified session and offers verification', async () => {
    const { target, component } = await render(false);
    expect(target.textContent).toContain('Not verified');
    expect(target.textContent).toContain('Verify this device');
    const button = [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Start verification');
    expect(button, 'the unverified session can start verification').toBeTruthy();
    button!.click();
    expect(mocks.onclose).toHaveBeenCalled();
    expect(mocks.requestVerification).toHaveBeenCalled();
    unmount(component);
    target.remove();
  });

  it('does not offer verification while the session state is unknown', async () => {
    // A failed read must not look like "not verified": the panel stays silent rather than inviting
    // a verification the user may not need.
    mocks.invoke.mockRejectedValue(new Error('engine offline'));
    const target = document.createElement('div');
    document.body.appendChild(target);
    const component = mount(Encryption, { target, props: { onclose: mocks.onclose } });
    flushSync();
    await vi.waitFor(() => expect(target.textContent).toContain('engine offline'));
    flushSync();
    expect(target.textContent).not.toContain('Start verification');
    unmount(component);
    target.remove();
  });
});
