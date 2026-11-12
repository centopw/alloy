import { describe, it, expect } from 'vitest';
import { session } from './session.svelte';

/** A pair shaped like the one my.telegram.org/apps shows. */
const API_HASH = '1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d';

describe('session: Telegram application pair', () => {
  it('clears the pair, reports Telegram unready with its setup, then stores a new one', async () => {
    await session.clearTelegramApp();
    expect(session.telegramApp).toEqual({ configured: false, apiId: null, source: 'none' });
    const unready = session.networks.find((info) => info.network === 'telegram');
    expect(unready?.ready).toBe(false);
    expect(unready?.setup?.kind).toBe('telegram_app');
    expect(unready?.setup?.app_url).toBe('https://my.telegram.org/apps');

    // The engine refuses a pair it cannot use; the store surfaces that and stores nothing.
    await expect(session.setTelegramApp(0, API_HASH)).rejects.toMatchObject({
      code: 'invalid_argument'
    });
    expect(session.telegramApp?.configured).toBe(false);

    await session.setTelegramApp(2040, API_HASH);
    expect(session.telegramApp).toEqual({ configured: true, apiId: 2040, source: 'runtime' });
    const ready = session.networks.find((info) => info.network === 'telegram');
    expect(ready?.ready).toBe(true);
    expect(ready?.setup).toBeNull();
  });

  it('refuses to start a Telegram login while the pair is missing', async () => {
    await session.clearTelegramApp();
    await expect(session.beginLogin('telegram')).rejects.toMatchObject({
      code: 'not_supported'
    });
    expect(session.login).toBeNull();

    // Storing the pair unblocks the same call without a restart.
    await session.setTelegramApp(2040, API_HASH);
    await session.beginLogin('telegram');
    expect(session.login?.step.type).toBe('phone_number');
    await session.cancelLogin();
  });
});
