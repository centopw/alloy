import { describe, it, expect, vi } from 'vitest';
import { applyDiff, AlloyClient } from './client.js';
import { MockTransport } from './mock.js';
import type { EngineEvent, VectorDiffDto } from '../generated/index.js';

describe('applyDiff - 11 VectorDiffDto variants', () => {
  it('handles Append', () => {
    const list = [1, 2];
    const diff: VectorDiffDto<number> = { type: 'Append', content: { values: [3, 4] } };
    applyDiff(list, diff);
    expect(list).toEqual([1, 2, 3, 4]);
  });

  it('handles Clear', () => {
    const list = [1, 2, 3];
    const diff: VectorDiffDto<number> = { type: 'Clear' };
    applyDiff(list, diff);
    expect(list).toEqual([]);
  });

  it('handles PushFront', () => {
    const list = [2, 3];
    const diff: VectorDiffDto<number> = { type: 'PushFront', content: { value: 1 } };
    applyDiff(list, diff);
    expect(list).toEqual([1, 2, 3]);
  });

  it('handles PushBack', () => {
    const list = [1, 2];
    const diff: VectorDiffDto<number> = { type: 'PushBack', content: { value: 3 } };
    applyDiff(list, diff);
    expect(list).toEqual([1, 2, 3]);
  });

  it('handles PopFront', () => {
    const list = [1, 2, 3];
    const diff: VectorDiffDto<number> = { type: 'PopFront' };
    applyDiff(list, diff);
    expect(list).toEqual([2, 3]);
  });

  it('handles PopBack', () => {
    const list = [1, 2, 3];
    const diff: VectorDiffDto<number> = { type: 'PopBack' };
    applyDiff(list, diff);
    expect(list).toEqual([1, 2]);
  });

  it('handles Insert', () => {
    const list = ['a', 'c'];
    const diff: VectorDiffDto<string> = { type: 'Insert', content: { index: 1, value: 'b' } };
    applyDiff(list, diff);
    expect(list).toEqual(['a', 'b', 'c']);
  });

  it('handles Set', () => {
    const list = ['a', 'b', 'c'];
    const diff: VectorDiffDto<string> = { type: 'Set', content: { index: 1, value: 'updated' } };
    applyDiff(list, diff);
    expect(list).toEqual(['a', 'updated', 'c']);
  });

  it('handles Remove', () => {
    const list = ['a', 'b', 'c'];
    const diff: VectorDiffDto<string> = { type: 'Remove', content: { index: 1 } };
    applyDiff(list, diff);
    expect(list).toEqual(['a', 'c']);
  });

  it('handles Truncate', () => {
    const list = [1, 2, 3, 4, 5];
    const diff: VectorDiffDto<number> = { type: 'Truncate', content: { length: 3 } };
    applyDiff(list, diff);
    expect(list).toEqual([1, 2, 3]);
  });

  it('handles Reset', () => {
    const list = [1, 2, 3];
    const diff: VectorDiffDto<number> = { type: 'Reset', content: { values: [9, 8, 7] } };
    applyDiff(list, diff);
    expect(list).toEqual([9, 8, 7]);
  });
});

describe('MockTransport & AlloyClient', () => {
  it('discovers homeserver and logs in', async () => {
    const mock = new MockTransport();
    const client = new AlloyClient(mock);

    const info = await client.discoverHomeserver('matrix.alloy.chat');
    expect(info).toEqual({
      type: 'HomeserverInfo',
      payload: {
        url: 'https://matrix.alloy.chat',
        name: 'Alloy Matrix Server',
        supports_oidc: true,
      },
    });

    const loginRes = await client.loginPassword('https://matrix.alloy.chat', 'alice', 'secret');
    expect(loginRes.type).toBe('LoggedIn');
  });

  it('streams room list diffs on subscription', async () => {
    const mock = new MockTransport();
    const client = new AlloyClient(mock);

    const receivedEvents: EngineEvent[] = [];
    const unsubscribe = client.subscribe((event) => {
      receivedEvents.push(event);
    });

    const subRes = await client.subscribeRoomList();
    expect(subRes.type).toBe('RoomListSubscribed');

    await vi.waitFor(() => {
      expect(receivedEvents.length).toBeGreaterThan(0);
    });

    const roomDiff = receivedEvents.find((e) => e.type === 'RoomListDiff');
    expect(roomDiff).toBeDefined();

    unsubscribe();
  });

  it('handles SendText optimistic local echo and Sent transition', async () => {
    const mock = new MockTransport();
    const client = new AlloyClient(mock);

    const events: EngineEvent[] = [];
    client.subscribe((ev) => events.push(ev));

    await client.subscribeTimeline('!general:alloy.chat');

    const sendRes = await client.sendText('!general:alloy.chat', 'Hello world');
    expect(sendRes.type).toBe('MessageSent');

    await vi.waitFor(
      () => {
        const tlDiffs = events.filter((e) => e.type === 'TimelineDiff');
        // Initial Reset + PushBack (local echo) + Set (transition to Sent)
        expect(tlDiffs.length).toBeGreaterThanOrEqual(3);
      },
      { timeout: 500 }
    );
  });

  it('handles device verification (SAS) flow', async () => {
    const mock = new MockTransport();
    const client = new AlloyClient(mock);

    const events: EngineEvent[] = [];
    client.subscribe((ev) => events.push(ev));

    const sasRes = await client.startSas('@alice:alloy.chat', 'DEV_1');
    expect(sasRes.type).toBe('SasStarted');

    await vi.waitFor(() => {
      const verEvent = events.find((e) => e.type === 'VerificationState');
      expect(verEvent).toBeDefined();
    });

    const confirmRes = await client.confirmSas('@alice:alloy.chat', 'DEV_1');
    expect(confirmRes.type).toBe('SasConfirmed');

    await vi.waitFor(() => {
      const verified = events.some(
        (e) => e.type === 'VerificationState' && e.payload.state === 'Verified'
      );
      expect(verified).toBe(true);
    });
  });
});
