import { describe, it, expect } from 'vitest';
import { MockTransport } from './mock.js';

async function ownProfile(mock: MockTransport) {
  const res = await mock.invoke({ action: 'GetOwnProfile' });
  if (res.type !== 'OwnProfile') throw new Error(`unexpected response ${res.type}`);
  return res.payload.profile;
}

async function listDevices(mock: MockTransport) {
  const res = await mock.invoke({ action: 'ListDevices' });
  if (res.type !== 'Devices') throw new Error(`unexpected response ${res.type}`);
  return res.payload.devices;
}

async function sessionInfo(mock: MockTransport) {
  const res = await mock.invoke({ action: 'GetSessionInfo' });
  if (res.type !== 'SessionInfo') throw new Error(`unexpected response ${res.type}`);
  return res.payload.info;
}

async function clearMediaCache(mock: MockTransport) {
  const res = await mock.invoke({ action: 'ClearMediaCache' });
  if (res.type !== 'MediaCacheCleared') throw new Error(`unexpected response ${res.type}`);
  return res.payload.freed_bytes;
}

describe('MockTransport settings commands', () => {
  it('starts with the default notification rules', async () => {
    const mock = new MockTransport();
    const res = await mock.invoke({ action: 'GetNotificationSettings' });
    expect(res).toEqual({
      type: 'NotificationSettings',
      payload: {
        settings: {
          one_to_one: 'All',
          group: 'Mentions',
          room_mention: true,
          user_mention: true,
          invite: true,
        },
      },
    });
  });

  it('updates the account-wide default and mention rules', async () => {
    const mock = new MockTransport();
    await mock.invoke({
      action: 'SetDefaultNotificationLevel',
      params: { one_to_one: false, level: 'Mute' },
    });
    await mock.invoke({
      action: 'SetMentionRules',
      params: { room_mention: false, user_mention: null, invite: null },
    });
    const res = await mock.invoke({ action: 'GetNotificationSettings' });
    expect(res).toEqual({
      type: 'NotificationSettings',
      payload: {
        settings: {
          one_to_one: 'All',
          group: 'Mute',
          room_mention: false,
          user_mention: true,
          invite: true,
        },
      },
    });
  });

  it('room level follows its category default until overridden', async () => {
    const mock = new MockTransport();
    const group = '!general:alloy.chat';
    const dm = '!bob:alloy.chat';
    const read = (roomId: string) =>
      mock.invoke({ action: 'GetRoomNotificationLevel', params: { room_id: roomId } });

    expect(await read(group)).toEqual({
      type: 'RoomNotificationLevel',
      payload: { room_id: group, level: 'Mentions', is_default: true },
    });
    expect(await read(dm)).toEqual({
      type: 'RoomNotificationLevel',
      payload: { room_id: dm, level: 'All', is_default: true },
    });

    await mock.invoke({
      action: 'SetRoomNotificationLevel',
      params: { room_id: group, level: 'Mute' },
    });
    expect(await read(group)).toEqual({
      type: 'RoomNotificationLevel',
      payload: { room_id: group, level: 'Mute', is_default: false },
    });

    await mock.invoke({
      action: 'SetDefaultNotificationLevel',
      params: { one_to_one: false, level: 'All' },
    });
    expect(await read(group)).toEqual({
      type: 'RoomNotificationLevel',
      payload: { room_id: group, level: 'Mute', is_default: false },
    });

    await mock.invoke({
      action: 'SetRoomNotificationLevel',
      params: { room_id: group, level: null },
    });
    expect(await read(group)).toEqual({
      type: 'RoomNotificationLevel',
      payload: { room_id: group, level: 'All', is_default: true },
    });
  });

  it('trims the display name and rejects an empty one', async () => {
    const mock = new MockTransport();
    await mock.invoke({ action: 'SetDisplayName', params: { name: '  Alicia  ' } });
    expect((await ownProfile(mock)).display_name).toBe('Alicia');

    await expect(
      mock.invoke({ action: 'SetDisplayName', params: { name: '   ' } }),
    ).rejects.toMatchObject({ code: 'invalid_argument' });

    expect((await ownProfile(mock)).display_name).toBe('Alicia');
  });

  it('sets and clears the own avatar', async () => {
    const mock = new MockTransport();
    expect((await ownProfile(mock)).avatar_url).toBeNull();

    await mock.invoke({ action: 'SetOwnAvatar', params: { file_path: '/tmp/avatar.png' } });
    expect((await ownProfile(mock)).avatar_url).toBe('mxc://alloy.chat/mock-own-avatar');

    await mock.invoke({ action: 'SetOwnAvatar', params: { file_path: null } });
    expect((await ownProfile(mock)).avatar_url).toBeNull();
  });

  it('lists the current verified device first, with one current session', async () => {
    const mock = new MockTransport();
    const profile = await ownProfile(mock);
    const devices = await listDevices(mock);

    expect(devices).toHaveLength(3);
    expect(devices.filter((d) => d.is_current)).toHaveLength(1);
    expect(devices[0]).toMatchObject({
      device_id: profile.device_id,
      is_current: true,
      verified: true,
    });
    expect(devices.some((d) => !d.verified)).toBe(true);
  });

  it('reports this session keys with cross-signing and backup ready', async () => {
    const mock = new MockTransport();
    const profile = await ownProfile(mock);
    const info = await sessionInfo(mock);

    expect(info.device_id).toBe(profile.device_id);
    expect(info.ed25519_key).not.toBeNull();
    expect(info.curve25519_key).not.toBeNull();
    expect(info.cross_signing_ready).toBe(true);
    expect(info.backup_enabled).toBe(true);
  });

  it('frees the media cache once and reports zero afterwards', async () => {
    const mock = new MockTransport();
    expect(await clearMediaCache(mock)).toBeGreaterThan(0);
    expect(await clearMediaCache(mock)).toBe(0);
  });
});
