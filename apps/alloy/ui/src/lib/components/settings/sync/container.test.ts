import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { EngineCommand, EngineEvent, EngineResponse, SyncStatusReport } from '$lib/generated';
import { t } from '$lib/i18n';

const engine = vi.hoisted(() => {
  const handlers = new Set<(event: EngineEvent) => void>();
  return {
    invoke: vi.fn(),
    subscribe: vi.fn((handler: (event: EngineEvent) => void) => {
      handlers.add(handler);
      return () => handlers.delete(handler);
    }),
    emit: (event: EngineEvent) => {
      for (const handler of handlers) handler(event);
    },
    handlers,
  };
});

vi.mock('$lib/ipc', () => ({
  client: { invoke: engine.invoke, subscribe: engine.subscribe },
}));

import DevicesSyncLive from './DevicesSyncLive.svelte';
import ContextMenu from '$lib/components/common/ContextMenu.svelte';

const teardown: Array<() => void> = [];

function status(overrides: Partial<SyncStatusReport> = {}): SyncStatusReport {
  return {
    mode: 'off',
    url: null,
    pin: null,
    token_configured: false,
    state: 'idle',
    last_sync_ms: null,
    last_error: null,
    layers: [],
    devices: [],
    ...overrides,
  };
}

function render(): HTMLElement {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(DevicesSyncLive, { target });
  teardown.push(() => unmount(instance));
  flushSync();
  return target;
}

async function settle() {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
  flushSync();
}

function button(target: ParentNode, label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll('button')].find(
    (entry) => entry.textContent?.trim() === label
  );
  if (!found) throw new Error(`no button “${label}”`);
  return found;
}

beforeEach(() => {
  engine.handlers.clear();
  engine.subscribe.mockClear();
  engine.invoke.mockReset();
  engine.invoke.mockImplementation(async (command: EngineCommand) => {
    switch (command.action) {
      case 'ListDevices':
        return {
          type: 'DevicesList',
          payload: {
            devices: [
              {
                device_id: 'demo-this',
                label: 'Laptop',
                enrolled_ms: 1,
                epoch: 2,
                current: true,
              },
            ],
          },
        };
      case 'SyncStatus':
        return { type: 'SyncStatus', payload: { status: status() } };
      default:
        throw new Error(`unexpected command ${command.action}`);
    }
  });
});

afterEach(() => {
  for (const destroy of teardown.splice(0)) destroy();
  document.body.replaceChildren();
});

describe('DevicesSyncLive pairing', () => {
  it('declines the SAS without accepting, then offers a restart', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [] } };
        case 'SyncStatus':
          return { type: 'SyncStatus', payload: { status: status() } };
        case 'BeginPairing':
          return {
            type: 'PairingStarted',
            payload: {
              pairing_id: 'p1',
              ticket: 'alloy://pair?p=t',
              code: 'AB34CD56',
              endpoint: '127.0.0.1:1',
              expires_at_ms: Date.now() + 60_000,
            },
          };
        case 'PairingStatus':
          return {
            type: 'PairingState',
            payload: {
              state: 'sas_ready',
              sas: '123456',
              peer_label: 'Pixel',
              transferred: 0,
              error: null,
            },
          };
        case 'ConfirmPairing':
          return {
            type: 'PairingState',
            payload: {
              state: 'declined',
              sas: null,
              peer_label: 'Pixel',
              transferred: 0,
              error: null,
            },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();

    button(target, t('sync-pair-start')).click();
    await settle();

    engine.emit({
      type: 'PairingChanged',
      payload: { pairing_id: 'p1', state: 'sas_ready', sas: '123456' },
    });
    flushSync();
    const digits = target.querySelector(`[aria-label="${t('sync-sas-label')}"]`);
    expect(digits?.textContent?.replace(/\s+/g, '')).toBe('123456');

    button(target, t('sync-sas-no-match')).click();
    await settle();

    const confirm = engine.invoke.mock.calls.find(
      ([command]) => command.action === 'ConfirmPairing'
    );
    expect(confirm?.[0]).toMatchObject({
      action: 'ConfirmPairing',
      params: { pairing_id: 'p1', accept: false },
    });
    const accepted = engine.invoke.mock.calls.some(([value]) => {
      return value.action === 'ConfirmPairing' && value.params?.accept === true;
    });
    expect(accepted, 'a decline never sends an acceptance').toBe(false);
    expect(target.textContent).toContain(t('sync-pairing-state-declined'));
    expect(target.textContent).toContain(t('sync-pairing-retry'));
  });

  it('shows an expired pairing with a restart', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [] } };
        case 'SyncStatus':
          return { type: 'SyncStatus', payload: { status: status() } };
        case 'BeginPairing':
          return {
            type: 'PairingStarted',
            payload: {
              pairing_id: 'p2',
              ticket: 'alloy://pair?p=t',
              code: 'AB34CD56',
              endpoint: '127.0.0.1:1',
              expires_at_ms: Date.now() + 60_000,
            },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();
    button(target, t('sync-pair-start')).click();
    await settle();

    engine.emit({
      type: 'PairingChanged',
      payload: { pairing_id: 'p2', state: 'expired', sas: null },
    });
    flushSync();

    expect(target.textContent).toContain(t('sync-pairing-state-expired'));
    expect(target.textContent).toContain(t('sync-pairing-retry'));
  });
});

describe('DevicesSyncLive server', () => {
  const STORED = 'aaaa1111';
  const PRESENTED = 'bbbb2222';

  function pinMismatchSetup() {
    engine.invoke.mockImplementation(
      async (command: EngineCommand) => {
        switch (command.action) {
          case 'ListDevices':
            return { type: 'DevicesList', payload: { devices: [] } };
          case 'SyncStatus':
            return {
              type: 'SyncStatus',
              payload: {
                status: status({
                  mode: 'prefs',
                  url: 'https://sync.example',
                  pin: STORED,
                  token_configured: true,
                }),
              },
            };
          case 'SyncNow':
            throw {
              code: 'pin_mismatch',
              message: `the sync endpoint certificate changed: pinned ${STORED}, presented ${PRESENTED}`,
              reason: 'pin_mismatch',
            };
          case 'ConfigureSync':
            return {
              type: 'SyncConfigured',
              payload: {
                pin: PRESENTED,
                status: status({
                  mode: 'prefs',
                  url: 'https://sync.example',
                  pin: PRESENTED,
                  token_configured: true,
                }),
              },
            };
          default:
            throw new Error(`unexpected command ${command.action}`);
        }
      }
    );
  }

  it('renders both fingerprints and trusts the new pin only on the explicit action', async () => {
    pinMismatchSetup();
    const target = render();
    await settle();

    button(target, t('sync-server-sync-now')).click();
    await settle();

    expect(target.textContent).toContain(t('sync-pin-mismatch-title'));
    const pins = [...target.querySelectorAll('.fingerprint code')].map((entry) =>
      entry.textContent?.trim()
    );
    expect(pins).toEqual(['AAAA 1111', 'BBBB 2222']);

    const configureCalls = () =>
      engine.invoke.mock.calls.filter(
        ([command]) => command.action === 'ConfigureSync'
      );
    expect(configureCalls(), 'nothing retries silently').toHaveLength(0);

    button(target, t('sync-pin-mismatch-trust')).click();
    await settle();

    expect(configureCalls()).toHaveLength(1);
    expect(configureCalls()[0][0]).toMatchObject({
      action: 'ConfigureSync',
      params: { url: 'https://sync.example', accept_new_pin: true },
    });
    expect(target.textContent).not.toContain(t('sync-pin-mismatch-title'));
  });
});

describe('DevicesSyncLive configuration bundle', () => {
  it('dry-runs an import and applies it only on confirm', async () => {
    engine.invoke.mockImplementation(
      async (command: EngineCommand) => {
        switch (command.action) {
          case 'ListDevices':
            return { type: 'DevicesList', payload: { devices: [] } };
          case 'SyncStatus':
            return { type: 'SyncStatus', payload: { status: status() } };
          case 'ExportConfigBundle':
            return {
              type: 'ConfigBundleExported',
              payload: {
                bundle_json: '{"schema":1}',
                bytes: 12,
                summary: {
                  settings: true,
                  catalogue: true,
                  tags: 2,
                  backgrounds: 0,
                  theme: null,
                },
              },
            };
          case 'ImportConfigBundle':
            return {
              type: 'ConfigBundleImported',
              payload: {
                report: {
                  settings: true,
                  catalogue: false,
                  tags: 2,
                  backgrounds: 0,
                  theme: false,
                },
                applied: command.params?.apply === true,
              },
            };
          default:
            throw new Error(`unexpected command ${command.action}`);
        }
      }
    );

    const target = render();
    await settle();

    button(target, t('sync-export')).click();
    await settle();
    expect(target.textContent).toContain(t('sync-export-summary-tags', { count: 2 }));

    const field = target.querySelector('textarea') as HTMLTextAreaElement | null;
    expect(field, 'the import field exists').toBeTruthy();
    field!.value = '{"schema":1}';
    field!.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();

    button(target, t('sync-import-check')).click();
    await settle();

    expect(target.textContent).toContain(
      t('sync-import-report', {
        changes: `${t('sync-import-part-settings')}, ${t('sync-import-part-tags', { count: 2 })}`,
      })
    );
    const importCalls = () =>
      engine.invoke.mock.calls.filter(
        ([command]) => command.action === 'ImportConfigBundle'
      );
    expect(importCalls()).toHaveLength(1);
    expect(importCalls()[0][0]).toMatchObject({ params: { apply: false } });
    expect(target.textContent, 'nothing is written before confirm').not.toContain(
      t('sync-import-applied')
    );

    button(target, t('sync-import-apply')).click();
    await settle();

    expect(importCalls()).toHaveLength(2);
    expect(importCalls()[1][0]).toMatchObject({ params: { apply: true } });
    expect(target.textContent).toContain(t('sync-import-applied'));
  });
});

describe('DevicesSyncLive fresh-device recovery', () => {
  const PIN = 'c0ffee11'.repeat(8);
  const RECOVERED_AT = Date.UTC(2026, 10, 17, 9, 30);
  const WORDS = Array.from({ length: 24 }, () => 'abandon');
  const THIS_DEVICE = {
    device_id: 'demo-this',
    label: 'Fresh laptop',
    enrolled_ms: 1,
    epoch: 0,
    current: true,
  };

  function typeInto(field: HTMLInputElement | HTMLTextAreaElement, value: string) {
    field.value = value;
    field.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
  }

  function recoveryForm(target: HTMLElement) {
    const form = target.querySelector('form.recover') as HTMLFormElement | null;
    expect(form, 'the fresh device offers the phrase recovery form').toBeTruthy();
    return form!;
  }

  it('recovers from the phrase, re-reads the roster and status, and renders the layers', async () => {
    let recovered = false;
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return {
            type: 'DevicesList',
            payload: {
              devices: recovered
                ? [
                    { ...THIS_DEVICE, label: 'Restored laptop' },
                    {
                      device_id: 'demo-peer',
                      label: 'Old phone',
                      enrolled_ms: 1,
                      epoch: 1,
                      current: false,
                    },
                  ]
                : [THIS_DEVICE],
            },
          };
        case 'SyncStatus':
          return {
            type: 'SyncStatus',
            payload: {
              status: status(
                recovered
                  ? {
                      mode: 'prefs',
                      url: 'https://sync.example',
                      pin: PIN,
                      token_configured: true,
                    }
                  : {}
              ),
            },
          };
        case 'RecoverFromPhrase':
          recovered = true;
          return {
            type: 'Recovered',
            payload: {
              layers: [
                { key: 'settings', enabled: true, updated_at_ms: RECOVERED_AT },
                { key: 'tags', enabled: true, updated_at_ms: RECOVERED_AT },
                { key: 'messages', enabled: false, updated_at_ms: 0 },
              ],
              devices: [
                { ...THIS_DEVICE, label: 'Restored laptop' },
                {
                  device_id: 'demo-peer',
                  label: 'Old phone',
                  enrolled_ms: 1,
                  epoch: 1,
                  current: false,
                },
              ],
            },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();

    expect(target.textContent).toContain(t('sync-recovery-recover-title'));
    expect(target.textContent).toContain(t('sync-recovery-pin-honesty'));

    const form = recoveryForm(target);
    typeInto(form.querySelector('textarea')!, WORDS.join(' '));
    typeInto(form.querySelector('input[type="url"]')!, 'https://sync.example');
    typeInto(form.querySelector('input[type="password"]')!, 'secret-token');

    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();

    const calls = (action: EngineCommand['action']) =>
      engine.invoke.mock.calls.filter(([command]) => command.action === action);
    expect(calls('RecoverFromPhrase')).toHaveLength(1);
    expect(calls('RecoverFromPhrase')[0][0]).toMatchObject({
      action: 'RecoverFromPhrase',
      params: {
        words: WORDS,
        url: 'https://sync.example',
        token: 'secret-token',
        accept_new_pin: false,
      },
    });
    expect(calls('ListDevices'), 'the roster is re-read').toHaveLength(2);
    expect(calls('SyncStatus'), 'the status is re-read').toHaveLength(2);

    // The recovered view is written once `load()` has answered, one turn after the status lands.
    await settle();

    const rendered = target.textContent ?? '';
    expect(rendered).toContain(t('sync-recovery-recovered-title'));
    expect(rendered).toContain(
      t('sync-recovery-roster', { devices: 'Restored laptop · Old phone' })
    );
    expect(rendered).toContain(
      t('sync-recovery-pin-recorded', { pin: 'C0FF EE11 C0FF EE11 C0FF EE11 C0FF EE11 C0FF EE11 C0FF EE11 C0FF EE11 C0FF EE11' })
    );

    const layers = [...target.querySelectorAll('.layer')].map((entry) => entry.textContent ?? '');
    expect(layers).toHaveLength(3);
    expect(layers[0]).toContain(t('sync-layer-settings'));
    expect(layers[0]).toContain(t('sync-layer-enabled'));
    expect(layers[0]).toContain(
      new Intl.DateTimeFormat('en', { dateStyle: 'medium', timeStyle: 'short' }).format(
        new Date(RECOVERED_AT)
      )
    );
    expect(layers[2]).toContain(t('sync-layer-disabled'));
    expect(layers[2]).toContain(t('sync-layer-never'));
  });

  it('keeps the typed words on recovery_rejected, writes nothing, and clears the token', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [THIS_DEVICE] } };
        case 'SyncStatus':
          return { type: 'SyncStatus', payload: { status: status() } };
        case 'RecoverFromPhrase':
          throw {
            code: 'recovery_rejected',
            message: 'the phrase does not open the escrow',
            reason: 'recovery_rejected',
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();

    const form = recoveryForm(target);
    const words = form.querySelector('textarea') as HTMLTextAreaElement;
    const token = form.querySelector('input[type="password"]') as HTMLInputElement;
    typeInto(words, WORDS.join(' '));
    typeInto(form.querySelector('input[type="url"]')!, 'https://sync.example');
    typeInto(token, 'secret-token');

    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();

    expect(target.textContent).toContain(t('sync-error-recovery-rejected'));
    expect(words.value, 'the typed phrase is kept so it can be fixed').toBe(WORDS.join(' '));
    expect(token.value, 'the token is write-only and never kept').toBe('');

    const calls = (action: EngineCommand['action']) =>
      engine.invoke.mock.calls.filter(([command]) => command.action === action);
    expect(calls('RecoverFromPhrase')).toHaveLength(1);
    expect(calls('ListDevices'), 'a rejected phrase writes nothing').toHaveLength(1);
    expect(calls('SyncStatus'), 'a rejected phrase writes nothing').toHaveLength(1);
    expect(target.textContent).not.toContain(t('sync-recovery-recovered-title'));
  });
});

describe('DevicesSyncLive fresh predicate', () => {
  function thisDevice(epoch: number) {
    return {
      device_id: 'demo-this',
      label: 'Laptop',
      enrolled_ms: 1,
      epoch,
      current: true,
    };
  }

  it('does not show the fresh form for a device with epoch 1 and an empty roster', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [thisDevice(1)] } };
        case 'SyncStatus':
          return {
            type: 'SyncStatus',
            payload: { status: status({ recovery_stale: true }) },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();

    expect(target.querySelector('form.recover'), 'an account device is not fresh').toBeNull();
    expect(target.textContent, 'the stale-phrase remedy stays reachable').toContain(
      t('sync-recovery-stale')
    );
  });

  it('shows the fresh form for a device with epoch 0, an empty roster and no phrase', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [thisDevice(0)] } };
        case 'SyncStatus':
          return { type: 'SyncStatus', payload: { status: status() } };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();

    expect(target.querySelector('form.recover'), 'a never-enrolled device is fresh').toBeTruthy();
    expect(target.textContent).toContain(t('sync-recovery-recover-title'));
  });
});

describe('DevicesSyncLive revoke result', () => {
  const THIS_DEVICE = {
    device_id: 'demo-this',
    label: 'Laptop',
    enrolled_ms: 1,
    epoch: 1,
    current: true,
  };
  const PEER = {
    device_id: 'demo-peer',
    label: 'Old phone',
    enrolled_ms: 1,
    epoch: 1,
    current: false,
  };

  function revokeSetup(revoked: Extract<EngineResponse, { type: 'DeviceRevoked' }>['payload']) {
    let remaining = true;
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return {
            type: 'DevicesList',
            payload: { devices: remaining ? [THIS_DEVICE, PEER] : [THIS_DEVICE] },
          };
        case 'SyncStatus':
          return { type: 'SyncStatus', payload: { status: status({ devices: [THIS_DEVICE] }) } };
        case 'RevokeDevice':
          remaining = false;
          return { type: 'DeviceRevoked', payload: revoked };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });
  }

  async function revoke(target: HTMLElement) {
    button(target, t('sync-devices-revoke')).click();
    await settle();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog, 'the confirmation is a dialog').toBeTruthy();
    const confirm = [...dialog!.querySelectorAll('button')].find(
      (entry) => entry.textContent?.trim() === t('sync-devices-revoke-confirm')
    );
    expect(confirm, 'the confirmation is offered').toBeTruthy();
    confirm!.click();
    await settle();
  }

  it('renders the blob leftovers and the old-data warning when the server kept ciphertext', async () => {
    revokeSetup({
      epoch: 4,
      re_sealed: true,
      blobs_deleted: 5,
      blobs_remaining: 2,
      recovery_stale: false,
      error: null,
    });

    const target = render();
    await settle();
    await revoke(target);

    const rendered = target.textContent ?? '';
    expect(rendered).toContain(t('sync-revoke-result-title', { device: 'Old phone', epoch: 4 }));
    expect(rendered).toContain(t('sync-revoke-resealed'));
    expect(rendered).toContain(t('sync-revoke-blobs-deleted', { count: 5 }));
    expect(rendered).toContain(t('sync-revoke-leftover', { count: 2 }));
    expect(rendered, 'the old ciphertext is not presented as protected').toContain(
      t('sync-revoke-retroactive')
    );
    expect(rendered, 'no clean sweep is claimed').not.toContain(t('sync-revoke-clean'));
  });

  it('reports an incomplete server half from the response error', async () => {
    revokeSetup({
      epoch: 4,
      re_sealed: false,
      blobs_deleted: 0,
      blobs_remaining: null,
      recovery_stale: false,
      error: 'delete timed out',
    });

    const target = render();
    await settle();
    await revoke(target);

    const rendered = target.textContent ?? '';
    expect(rendered).toContain(t('sync-revoke-not-resealed'));
    expect(rendered).toContain(t('sync-revoke-error', { message: 'delete timed out' }));
    expect(rendered).toContain(t('sync-revoke-retroactive'));
    expect(rendered).not.toContain(t('sync-revoke-clean'));
  });

  it('shows no warning when the server reported every old blob gone', async () => {
    revokeSetup({
      epoch: 4,
      re_sealed: true,
      blobs_deleted: 5,
      blobs_remaining: 0,
      recovery_stale: false,
      error: null,
    });

    const target = render();
    await settle();
    await revoke(target);

    const rendered = target.textContent ?? '';
    expect(rendered).toContain(t('sync-revoke-clean'));
    expect(rendered).not.toContain(t('sync-revoke-leftover', { count: 2 }));
    expect(rendered).not.toContain(t('sync-revoke-retroactive'));
    expect(target.querySelectorAll('.revoke-result .warning')).toHaveLength(0);
  });
});

describe('DevicesSyncLive stale recovery phrase', () => {
  it('shows the remedy and clears the flag after CreateRecoveryPhrase', async () => {
    let stale = true;
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [] } };
        case 'SyncStatus':
          return {
            type: 'SyncStatus',
            payload: { status: status({ recovery_stale: stale }) },
          };
        case 'CreateRecoveryPhrase':
          stale = false;
          return {
            type: 'RecoveryPhraseCreated',
            payload: { words: Array.from({ length: 24 }, () => 'abandon') },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();

    expect(target.textContent).toContain(t('sync-recovery-stale'));
    button(target, t('sync-recovery-stale-create')).click();
    // CreateRecoveryPhrase, then the SyncStatus re-read that clears the flag, then the words.
    await settle();
    await settle();

    const calls = (action: EngineCommand['action']) =>
      engine.invoke.mock.calls.filter(([command]) => command.action === action);
    expect(calls('CreateRecoveryPhrase'), 'the remedy is the create command').toHaveLength(1);
    expect(calls('SyncStatus'), 'the status is re-read after creating').toHaveLength(2);
    expect(target.textContent).toContain(t('sync-recovery-created-title'));
    expect(target.textContent, 'the stale flag is gone').not.toContain(t('sync-recovery-stale'));
  });
});

describe('DevicesSyncLive message mirror', () => {
  const MIRRORED_AT = 1_700_000_000_000;
  const LAYERS: SyncStatusReport['layers'] = [
    { key: 'settings', enabled: true, updated_at_ms: MIRRORED_AT },
    { key: 'messages', enabled: true, updated_at_ms: MIRRORED_AT, windowed: true },
  ];

  /** The mode control is the shared Select: a trigger button plus a portal-backed menu. */
  function renderWithMenu(): HTMLElement {
    const menuTarget = document.createElement('div');
    document.body.appendChild(menuTarget);
    const menu = mount(ContextMenu, { target: menuTarget });
    teardown.push(() => unmount(menu));
    return render();
  }

  function chooseMode(target: HTMLElement, label: string) {
    const trigger = target.querySelector('.select-trigger') as HTMLButtonElement | null;
    expect(trigger, 'the mode control is a select trigger').toBeTruthy();
    trigger!.click();
    flushSync();
    const option = [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find(
      (entry) => entry.textContent?.trim() === label
    );
    if (!option) throw new Error(`no mode option “${label}”`);
    option.click();
    flushSync();
  }

  function typeIntoField(target: HTMLElement, selector: string, value: string) {
    const field = target.querySelector(selector) as HTMLInputElement | null;
    expect(field, `field ${selector}`).toBeTruthy();
    field!.value = value;
    field!.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
  }

  it('confirms Full before it applies and reports the windowed mirror', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [] } };
        case 'SyncStatus':
          return {
            type: 'SyncStatus',
            payload: {
              status: status({
                mode: 'prefs',
                url: 'https://sync.example',
                layers: [
                  { key: 'settings', enabled: true, updated_at_ms: MIRRORED_AT },
                  { key: 'messages', enabled: false, updated_at_ms: 0 },
                ],
              }),
            },
          };
        case 'ConfigureSync':
          return {
            type: 'SyncConfigured',
            payload: {
              pin: null,
              status: status({ mode: 'full', url: 'https://sync.example', layers: LAYERS }),
            },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = renderWithMenu();
    await settle();

    const calls = (action: EngineCommand['action']) =>
      engine.invoke.mock.calls.filter(([command]) => command.action === action);
    const trigger = target.querySelector('.select-trigger') as HTMLButtonElement;
    expect(trigger.textContent, 'Full is not the default').toContain(
      t('sync-server-mode-prefs')
    );
    expect(target.textContent, 'Prefs shows no mirror row').not.toContain(
      t('sync-layer-messages')
    );

    chooseMode(target, t('sync-server-mode-full'));
    expect(target.textContent).toContain(t('sync-server-full-confirm-title'));
    expect(target.textContent).toContain(t('sync-server-full-confirm-ciphertext'));
    expect(target.textContent).toContain(t('sync-server-full-confirm-loss'));
    expect(target.textContent).toContain(t('sync-server-full-confirm-revoke'));
    expect(target.textContent).toContain(t('sync-server-full-confirm-media'));
    expect(calls('ConfigureSync'), 'nothing is sent before the confirm').toHaveLength(0);

    typeIntoField(target, 'input[type="url"]', 'https://sync.example');
    button(target, t('sync-server-full-confirm-apply')).click();
    await settle();

    expect(calls('ConfigureSync')).toHaveLength(1);
    expect(calls('ConfigureSync')[0][0]).toMatchObject({
      action: 'ConfigureSync',
      params: { mode: 'full', url: 'https://sync.example' },
    });
    expect(target.textContent).toContain(t('sync-layer-messages'));
    expect(target.textContent).toContain(t('sync-layer-windowed'));
  });

  it('cancelling Full keeps the applied mode and sends nothing', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [] } };
        case 'SyncStatus':
          return {
            type: 'SyncStatus',
            payload: { status: status({ mode: 'prefs', url: 'https://sync.example' }) },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = renderWithMenu();
    await settle();

    chooseMode(target, t('sync-server-mode-full'));
    button(target, t('sync-server-full-confirm-cancel')).click();
    await settle();

    const calls = engine.invoke.mock.calls.filter(
      ([command]) => command.action === 'ConfigureSync'
    );
    expect(calls).toHaveLength(0);
    expect(
      (target.querySelector('.select-trigger') as HTMLButtonElement).textContent
    ).toContain(t('sync-server-mode-prefs'));
    expect(target.textContent).not.toContain(t('sync-server-full-confirm-title'));
  });

  it('Off renders no mirror row', async () => {
    engine.invoke.mockImplementation(async (command: EngineCommand) => {
      switch (command.action) {
        case 'ListDevices':
          return { type: 'DevicesList', payload: { devices: [] } };
        case 'SyncStatus':
          return {
            type: 'SyncStatus',
            payload: {
              status: status({
                mode: 'off',
                layers: [
                  { key: 'settings', enabled: false, updated_at_ms: 0 },
                  { key: 'messages', enabled: false, updated_at_ms: 0 },
                ],
              }),
            },
          };
        default:
          throw new Error(`unexpected command ${command.action}`);
      }
    });

    const target = render();
    await settle();

    expect(target.textContent).not.toContain(t('sync-layer-messages'));
    expect(target.querySelector('.server .layer')).toBeNull();
  });
});
