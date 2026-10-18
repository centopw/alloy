import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NotificationSettingsStore } from './notificationSettings.svelte';

const invoke = vi.fn();
vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

const initial = {
  one_to_one: 'All',
  group: 'All',
  room_mention: true,
  user_mention: true,
  invite: true,
};

const settingsRes = { type: 'NotificationSettings', payload: { settings: initial } };

let store: NotificationSettingsStore;

beforeEach(() => {
  invoke.mockReset();
  store = new NotificationSettingsStore();
});

describe('NotificationSettingsStore', () => {
  it('loads the account settings from the engine', async () => {
    invoke.mockResolvedValue(settingsRes);
    await store.load();
    expect(invoke).toHaveBeenCalledWith({ action: 'GetNotificationSettings' });
    expect(store.settings).toEqual(initial);
    expect(store.error).toBeNull();
  });

  it('records a load failure without throwing', async () => {
    invoke.mockRejectedValue(new Error('offline'));
    await store.load();
    expect(store.settings).toBeNull();
    expect(store.error).toBe('offline');
  });

  it('sends the default level for direct chats and applies it optimistically', async () => {
    invoke.mockResolvedValueOnce(settingsRes);
    await store.load();
    invoke.mockResolvedValueOnce({ type: 'Unit' });
    await store.setDefault(true, 'Mentions');
    expect(invoke).toHaveBeenLastCalledWith({
      action: 'SetDefaultNotificationLevel',
      params: { one_to_one: true, level: 'Mentions' },
    });
    expect(store.settings?.one_to_one).toBe('Mentions');
    expect(store.settings?.group).toBe('All');
    expect(store.saving).toBe(0);
  });

  it('rolls a rejected default change back to the confirmed state', async () => {
    invoke.mockResolvedValueOnce(settingsRes);
    await store.load();
    invoke.mockRejectedValueOnce(new Error('rejected'));
    await store.setDefault(false, 'Mentions');
    expect(store.settings?.group).toBe('All');
    expect(store.error).toBe('rejected');
  });

  it('sends only the changed mention rule and leaves the others as null', async () => {
    invoke.mockResolvedValueOnce(settingsRes);
    await store.load();
    invoke.mockResolvedValueOnce({ type: 'Unit' });
    await store.setMentionRule('invite', false);
    expect(invoke).toHaveBeenLastCalledWith({
      action: 'SetMentionRules',
      params: { room_mention: null, user_mention: null, invite: false },
    });
    expect(store.settings?.invite).toBe(false);
    expect(store.settings?.room_mention).toBe(true);
  });

  it('does nothing for a write before the settings are loaded', async () => {
    await store.setMentionRule('room_mention', false);
    expect(invoke).not.toHaveBeenCalled();
  });
});
