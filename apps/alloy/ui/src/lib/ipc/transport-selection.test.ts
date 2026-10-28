import { describe, expect, it } from 'vitest';
import { chooseTransport, openTransport } from './index.js';
import { NoEngineError } from './transport.js';

/**
 * The shipped app must never invent data. These tests pin the boundary: demo content is reachable
 * only from a development build, and a browser build with no engine fails with one typed error.
 */
describe('transport selection', () => {
  const base = { tauri: false, bridge: null, token: null, dev: false };

  it('uses Tauri IPC in the packaged app, whatever the query string says', () => {
    expect(chooseTransport({ ...base, tauri: true, bridge: 'ws://127.0.0.1:8766', dev: false })).toEqual({
      kind: 'tauri',
    });
  });

  it('uses the bridge WebSocket when the page was pointed at one', () => {
    expect(
      chooseTransport({ ...base, bridge: 'ws://127.0.0.1:8766', token: 'secret' })
    ).toEqual({ kind: 'ws', url: 'ws://127.0.0.1:8766/?token=secret' });
  });

  it('keeps the bridge token out of the URL when none was given', () => {
    expect(chooseTransport({ ...base, bridge: 'ws://127.0.0.1:8766' })).toEqual({
      kind: 'ws',
      url: 'ws://127.0.0.1:8766/',
    });
  });

  it('does not turn a malformed ?engine= into demo data', () => {
    expect(chooseTransport({ ...base, bridge: 'not a url' })).toEqual({ kind: 'none' });
  });

  it('offers the in-memory demo engine to development builds only', () => {
    expect(chooseTransport({ ...base, dev: true })).toEqual({ kind: 'mock' });
    expect(chooseTransport({ ...base, dev: false })).toEqual({ kind: 'none' });
  });

  it('serves demo data in a development build', async () => {
    const transport = await openTransport(chooseTransport({ ...base, dev: true }));
    await expect(transport.invoke({ action: 'ListAccounts' })).resolves.toMatchObject({
      type: 'AccountsList',
    });
  });

  it('fails the shipped no-engine path with a typed error', async () => {
    const transport = await openTransport(chooseTransport({ ...base }));
    await expect(transport.invoke({ action: 'ListAccounts' })).rejects.toBeInstanceOf(NoEngineError);
    await expect(transport.invoke({ action: 'ListAccounts' })).rejects.toMatchObject({
      code: 'no_engine',
    });
  });
});
