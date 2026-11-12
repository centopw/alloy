import { describe, it, expect, vi } from 'vitest';
import { MockTransport } from './mock.js';
import { DEMO_ROOM } from '../mock/seed.js';
import type { EngineEvent, EngineResponse } from '../generated/index.js';

function expectResponse<T extends EngineResponse['type']>(
  res: EngineResponse,
  type: T
): Extract<EngineResponse, { type: T }> {
  if (res.type !== type) throw new Error(`expected ${type}, got ${res.type}`);
  return res as Extract<EngineResponse, { type: T }>;
}

async function settings(mock: MockTransport): Promise<Record<string, unknown>> {
  const res = expectResponse(await mock.invoke({ action: 'GetSettings' }), 'SettingsRetrieved');
  return JSON.parse(res.payload.settings_json) as Record<string, unknown>;
}

describe('MockTransport settings and themes', () => {
  it('starts with the seeded settings and stores a new key as JSON', async () => {
    const mock = new MockTransport();
    expect(await settings(mock)).toEqual({ notificationLevel: 'all', compactRows: false });

    const set = expectResponse(
      await mock.invoke({ action: 'SetSetting', params: { key: 'fontScale', value_json: '120' } }),
      'SettingSet'
    );
    expect(set.payload.key).toBe('fontScale');
    expect(await settings(mock)).toEqual({ notificationLevel: 'all', compactRows: false, fontScale: 120 });
  });

  it('refuses a setting value that is not JSON', async () => {
    const mock = new MockTransport();
    await expect(
      mock.invoke({ action: 'SetSetting', params: { key: 'fontScale', value_json: '120%' } })
    ).rejects.toMatchObject({ code: 'invalid_argument' });
    expect(await settings(mock)).toEqual({ notificationLevel: 'all', compactRows: false });
  });

  it('lists the themes and returns the CSS of one', async () => {
    const mock = new MockTransport();

    const themes = expectResponse(await mock.invoke({ action: 'ListThemes' }), 'ThemesList').payload.themes;
    expect(themes).toEqual([
      ['dark', 'Dark'],
      ['light', 'Light'],
      ['amoled', 'AMOLED'],
    ]);

    const css = expectResponse(await mock.invoke({ action: 'GetTheme', params: { id: 'amoled' } }), 'ThemeCss');
    expect(css.payload.css).toContain('--alloy-theme-id: "amoled"');

    await expect(mock.invoke({ action: 'GetTheme', params: { id: 'sepia' } })).rejects.toMatchObject({
      code: 'not_found',
    });
  });

  it('activates a known theme and rejects an unknown one', async () => {
    const mock = new MockTransport();

    expect((await mock.invoke({ action: 'SetActiveTheme', params: { id: 'light' } })).type).toBe('ThemeSetActive');
    await expect(mock.invoke({ action: 'SetActiveTheme', params: { id: 'sepia' } })).rejects.toMatchObject({
      code: 'not_found',
    });
  });

  it('answers the media-cache clear and the manual sync refresh', async () => {
    const mock = new MockTransport();
    expect((await mock.invoke({ action: 'ClearMediaCache' })).type).toBe('MediaCacheCleared');
    expect((await mock.invoke({ action: 'RefreshSync' })).type).toBe('SyncRefreshed');
  });

  it('reconnects every account on refresh and reports each transition', async () => {
    const mock = new MockTransport();
    const events: EngineEvent[] = [];
    mock.subscribe((event) => events.push(event));
    const accounts = expectResponse(await mock.invoke({ action: 'ListAccounts' }), 'AccountsList').payload.accounts;

    await mock.invoke({ action: 'RefreshSync' });

    await vi.waitFor(() => {
      const connecting = events.filter(
        (event) => event.type === 'ConnectionStatus' && event.payload.status === 'Connecting'
      );
      expect(connecting).toHaveLength(accounts.length);
    });
    await vi.waitFor(() => {
      const connected = events.filter(
        (event) => event.type === 'ConnectionStatus' && event.payload.status === 'Connected'
      );
      expect(new Set(connected.map((event) => (event.type === 'ConnectionStatus' ? event.payload.account_id : '')))).toEqual(
        new Set(accounts.map((account) => account.id))
      );
    });
  });

  it('announces the connectors to the first subscriber', async () => {
    const mock = new MockTransport();
    const events: EngineEvent[] = [];
    mock.subscribe((event) => events.push(event));

    await vi.waitFor(() => {
      const syncing = events.filter(
        (event) => event.type === 'ConnectionStatus' && event.payload.status === 'Syncing'
      );
      expect(syncing).toHaveLength(2);
    });
    await vi.waitFor(() => {
      const connected = events.filter(
        (event) => event.type === 'ConnectionStatus' && event.payload.status === 'Connected'
      );
      expect(connected).toHaveLength(2);
    });
  });

  it('stops streaming room-list diffs once the subscription is unsubscribed', async () => {
    const mock = new MockTransport();
    const diffs: EngineEvent[] = [];
    const subscribed = expectResponse(
      await mock.invoke({ action: 'SubscribeRoomList', params: { filter: null } }),
      'RoomListSubscribed'
    );
    const subscriptionId = subscribed.payload.subscription_id;
    mock.subscribe((event) => {
      if (event.type === 'RoomListDiff' && event.payload.subscription_id === subscriptionId) {
        diffs.push(event);
      }
    });
    await vi.waitFor(() => expect(diffs).toHaveLength(1));

    expect((await mock.invoke({ action: 'Unsubscribe', params: { subscription_id: subscriptionId } })).type).toBe(
      'Unsubscribed'
    );
    await mock.invoke({ action: 'SetRoomPinned', params: { room_id: DEMO_ROOM.telegramSam, pinned: true } });
    expect(diffs).toHaveLength(1);
  });
});
