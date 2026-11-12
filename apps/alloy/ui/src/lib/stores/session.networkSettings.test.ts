import { describe, it, expect } from 'vitest';
import { session } from './session.svelte';

/** The setting `key` of `network` as the last `ListNetworks` reported it. */
function setting(network: 'telegram' | 'messenger' | 'fake', key: string) {
  return session.networks
    .find((info) => info.network === network)
    ?.setup?.settings.find((entry) => entry.key === key);
}

describe('session: network runtime settings', () => {
  it('stores a Messenger proxy so it shows up in the network list, then clears it', async () => {
    await session.loadNetworks();
    expect(setting('messenger', 'proxy')?.value).toBeNull();

    await session.setNetworkSetting('messenger', 'proxy', 'socks5://127.0.0.1:1080');
    expect(setting('messenger', 'proxy')?.value).toBe('socks5://127.0.0.1:1080');

    await session.clearNetworkSetting('messenger', 'proxy');
    expect(setting('messenger', 'proxy')?.value).toBeNull();
  });

  it('declares the Messenger platform choice and restores its default on clear', async () => {
    await session.loadNetworks();
    const platform = setting('messenger', 'platform');
    expect(platform?.kind).toBe('choice');
    expect(platform?.options.map((option) => option.value)).toEqual(['android', 'ios']);

    await session.setNetworkSetting('messenger', 'platform', 'ios');
    expect(setting('messenger', 'platform')?.value).toBe('ios');

    await session.clearNetworkSetting('messenger', 'platform');
    expect(setting('messenger', 'platform')?.value).toBe('android');
  });

  it('surfaces the engine refusal of a value the declaration rejects', async () => {
    await expect(
      session.setNetworkSetting('messenger', 'proxy', 'ftp://example.com:21')
    ).rejects.toMatchObject({ code: 'invalid_argument' });
    expect(setting('messenger', 'proxy')?.value).toBeNull();

    // A key the network never declared is the same class of failure, not a silent no-op.
    await expect(
      session.setNetworkSetting('messenger', 'unknown', 'value')
    ).rejects.toMatchObject({ code: 'invalid_argument' });
  });
});
