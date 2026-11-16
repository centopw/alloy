import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { EngineCommand, EngineEvent, SyncStatusReport } from '$lib/generated';
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
