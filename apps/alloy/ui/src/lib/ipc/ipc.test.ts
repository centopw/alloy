import { describe, it, expect, vi } from 'vitest';
import { applyDiff, AlloyClient } from './client.js';
import { MockTransport } from './mock.js';
import { DEMO_ACCOUNT, DEMO_ROOM } from '../mock/seed.js';
import type {
  EngineEvent,
  EngineResponse,
  RoomId,
  RoomSummary,
  TimelineItemDto,
  VectorDiffDto,
} from '../generated/index.js';

function expectResponse<T extends EngineResponse['type']>(
  res: EngineResponse,
  type: T
): Extract<EngineResponse, { type: T }> {
  if (res.type !== type) throw new Error(`expected ${type}, got ${res.type}`);
  return res as Extract<EngineResponse, { type: T }>;
}

function harness() {
  const mock = new MockTransport();
  const events: EngineEvent[] = [];
  const client = new AlloyClient(mock);
  client.subscribe((event) => events.push(event));
  return { mock, client, events };
}

/** Opens the room list and applies every diff, exactly as a page would. */
async function openRoomList(client: AlloyClient, filter: string | null = null) {
  const res = expectResponse(await client.subscribeRoomList(filter), 'RoomListSubscribed');
  const subscriptionId = res.payload.subscription_id;
  const rows: RoomSummary[] = [];
  const stop = client.subscribe((event) => {
    if (event.type === 'RoomListDiff' && event.payload.subscription_id === subscriptionId) {
      applyDiff(rows, event.payload.diff);
    }
  });
  await vi.waitFor(() => expect(rows.length).toBeGreaterThan(0));
  return { subscriptionId, rows, stop };
}

async function openTimeline(client: AlloyClient, roomId: RoomId) {
  const res = expectResponse(await client.subscribeTimeline(roomId), 'TimelineSubscribed');
  const subscriptionId = res.payload.subscription_id;
  const items: TimelineItemDto[] = [];
  const stop = client.subscribe((event) => {
    if (event.type === 'TimelineDiff' && event.payload.subscription_id === subscriptionId) {
      applyDiff(items, event.payload.diff);
    }
  });
  await vi.waitFor(() => expect(items.length).toBeGreaterThan(0));
  return { subscriptionId, items, stop };
}

function lastMessage(items: TimelineItemDto[]): TimelineItemDto {
  const message = [...items].reverse().find((item) => item.content.kind !== 'DayDivider');
  if (!message) throw new Error('timeline has no message');
  return message;
}

function textBody(item: TimelineItemDto): string | null {
  return item.content.kind === 'TextMessage' ? item.content.data.body : null;
}

describe('applyDiff - 11 VectorDiffDto variants', () => {
  const cases: Array<{ name: string; start: string[]; diff: VectorDiffDto<string>; expected: string[] }> = [
    { name: 'Append', start: ['a'], diff: { type: 'Append', content: { values: ['b', 'c'] } }, expected: ['a', 'b', 'c'] },
    { name: 'Clear', start: ['a', 'b'], diff: { type: 'Clear' }, expected: [] },
    { name: 'PushFront', start: ['b'], diff: { type: 'PushFront', content: { value: 'a' } }, expected: ['a', 'b'] },
    { name: 'PushBack', start: ['a'], diff: { type: 'PushBack', content: { value: 'b' } }, expected: ['a', 'b'] },
    { name: 'PopFront', start: ['a', 'b'], diff: { type: 'PopFront' }, expected: ['b'] },
    { name: 'PopBack', start: ['a', 'b'], diff: { type: 'PopBack' }, expected: ['a'] },
    { name: 'Insert', start: ['a', 'c'], diff: { type: 'Insert', content: { index: 1, value: 'b' } }, expected: ['a', 'b', 'c'] },
    { name: 'Set', start: ['a', 'b'], diff: { type: 'Set', content: { index: 1, value: 'z' } }, expected: ['a', 'z'] },
    { name: 'Remove', start: ['a', 'b', 'c'], diff: { type: 'Remove', content: { index: 1 } }, expected: ['a', 'c'] },
    { name: 'Truncate', start: ['a', 'b', 'c'], diff: { type: 'Truncate', content: { length: 2 } }, expected: ['a', 'b'] },
    { name: 'Reset', start: ['a', 'b'], diff: { type: 'Reset', content: { values: ['x'] } }, expected: ['x'] },
  ];

  it.each(cases)('applies $name', ({ start, diff, expected }) => {
    const target = [...start];
    applyDiff(target, diff);
    expect(target).toEqual(expected);
  });

  it('ignores an out-of-range Set or Remove instead of corrupting the list', () => {
    const target = ['a'];
    applyDiff(target, { type: 'Set', content: { index: 4, value: 'z' } });
    applyDiff(target, { type: 'Remove', content: { index: 4 } });
    expect(target).toEqual(['a']);
  });
});

describe('accounts and login', () => {
  it('lists both seeded accounts and the networks this build can connect', async () => {
    const { client } = harness();

    const accounts = expectResponse(await client.listAccounts(), 'AccountsList').payload.accounts;
    expect(accounts.map((account) => account.network).sort()).toEqual(['messenger', 'telegram']);
    expect(accounts.every((account) => account.state === 'active')).toBe(true);

    const networks = expectResponse(await client.listNetworks(), 'Networks').payload.networks;
    expect(networks.map((info) => info.network)).toEqual(['telegram', 'messenger', 'fake']);
    // The demo starts with a stored Telegram pair, so every listed network can start a login.
    expect(networks.every((info) => info.ready && info.setup === null)).toBe(true);
  });

  it('walks Telegram phone → code → done when the number has no two-step password', async () => {
    const { client, events } = harness();

    const begin = expectResponse(await client.beginLogin('telegram'), 'LoginStep');
    expect(begin.payload.step.type).toBe('phone_number');

    const code = expectResponse(
      await client.submitLogin(begin.payload.login_id, { type: 'phone_number', phone: '+15550100' }),
      'LoginStep'
    );
    expect(code.payload.step).toEqual({ type: 'code', via: 'sms', length: 5 });

    const done = expectResponse(
      await client.submitLogin(begin.payload.login_id, { type: 'code', code: '12345' }),
      'LoginStep'
    );
    expect(done.payload.step.type).toBe('done');
    if (done.payload.step.type !== 'done') return;
    expect(done.payload.step.account.network).toBe('telegram');
    expect(done.payload.step.account.remote_user_id).toBe('tg:15550100');

    const accounts = expectResponse(await client.listAccounts(), 'AccountsList').payload.accounts;
    expect(accounts).toHaveLength(3);
    expect(events.some((event) => event.type === 'AccountsChanged')).toBe(true);
  });

  it('asks for the two-step password only when the phone ends with 9, and rejects a wrong code', async () => {
    const { client } = harness();

    const begin = expectResponse(await client.beginLogin('telegram'), 'LoginStep');
    await client.submitLogin(begin.payload.login_id, { type: 'phone_number', phone: '+155501009' });

    const wrong = expectResponse(
      await client.submitLogin(begin.payload.login_id, { type: 'code', code: '00000' }),
      'LoginStep'
    );
    expect(wrong.payload.step).toEqual({
      type: 'failed',
      reason: { type: 'invalid_input', field: 'code', message: 'That code is not the one the network sent.' },
    });

    const retry = expectResponse(await client.beginLogin('telegram'), 'LoginStep');
    await client.submitLogin(retry.payload.login_id, { type: 'phone_number', phone: '+155501009' });
    const password = expectResponse(
      await client.submitLogin(retry.payload.login_id, { type: 'code', code: '12345' }),
      'LoginStep'
    );
    expect(password.payload.step.type).toBe('password');

    const empty = expectResponse(
      await client.submitLogin(retry.payload.login_id, { type: 'password', password: '  ' }),
      'LoginStep'
    );
    expect(empty.payload.step.type).toBe('failed');
    if (empty.payload.step.type !== 'failed') return;
    expect(empty.payload.step.reason).toMatchObject({ type: 'invalid_input', field: 'password' });

    const done = expectResponse(await client.beginLogin('telegram'), 'LoginStep');
    await client.submitLogin(done.payload.login_id, { type: 'phone_number', phone: '+155501009' });
    await client.submitLogin(done.payload.login_id, { type: 'code', code: '12345' });
    const finished = expectResponse(
      await client.submitLogin(done.payload.login_id, { type: 'password', password: 'correct horse' }),
      'LoginStep'
    );
    expect(finished.payload.step.type).toBe('done');
  });

  it('walks Messenger cookies and refuses to continue without a required cookie', async () => {
    const { client } = harness();

    const begin = expectResponse(await client.beginLogin('messenger'), 'LoginStep');
    expect(begin.payload.step.type).toBe('cookies');
    if (begin.payload.step.type !== 'cookies') return;
    expect(begin.payload.step.fields.find((field) => field.name === 'c_user')?.required).toBe(true);

    const missing = expectResponse(
      await client.submitLogin(begin.payload.login_id, { type: 'cookies', values: { c_user: ' ', xs: 'token' } }),
      'LoginStep'
    );
    expect(missing.payload.step.type).toBe('failed');
    if (missing.payload.step.type !== 'failed') return;
    expect(missing.payload.step.reason).toMatchObject({ type: 'invalid_input', field: 'c_user' });

    const retry = expectResponse(await client.beginLogin('messenger'), 'LoginStep');
    const done = expectResponse(
      await client.submitLogin(retry.payload.login_id, {
        type: 'cookies',
        values: { c_user: '100012345', xs: 'session-token' },
      }),
      'LoginStep'
    );
    expect(done.payload.step.type).toBe('done');
    if (done.payload.step.type !== 'done') return;
    expect(done.payload.step.account.remote_user_id).toBe('fb:100012345');
  });

  it('abandons an attempt and refuses to submit to it afterwards', async () => {
    const { client } = harness();
    const begin = expectResponse(await client.beginLogin('telegram'), 'LoginStep');

    expect((await client.cancelLogin(begin.payload.login_id)).type).toBe('LoginCancelled');
    await expect(
      client.submitLogin(begin.payload.login_id, { type: 'phone_number', phone: '+15550100' })
    ).rejects.toMatchObject({ code: 'not_found' });
  });

  it('removes an account and its chats on logout', async () => {
    const { client, events } = harness();
    const { rows } = await openRoomList(client);

    expectResponse(await client.logout(DEMO_ACCOUNT.messenger), 'LoggedOut');

    const accounts = expectResponse(await client.listAccounts(), 'AccountsList').payload.accounts;
    expect(accounts.map((account) => account.network)).toEqual(['telegram']);
    expect(rows.every((row) => row.network === 'telegram')).toBe(true);
    expect(events.some((event) => event.type === 'AccountsChanged')).toBe(true);
  });
});

describe('room list stream', () => {
  it('streams both networks with pinned chats first', async () => {
    const { client } = harness();
    const { rows } = await openRoomList(client);

    expect(new Set(rows.map((row) => row.network))).toEqual(new Set(['telegram', 'messenger']));
    expect(rows[0].pinned).toBe(true);
    expect(rows[0].name).toBe('Dev Circle');
    expect(rows.at(-1)?.archived).toBe(true);
  });

  it('narrows the projection by chat name', async () => {
    const { client } = harness();
    const { rows } = await openRoomList(client, 'lena');

    expect(rows.map((row) => row.name)).toEqual(['Lena Fischer']);
  });

  it('emits diffs for pin, archive, mute, marked-unread and read changes', async () => {
    const { client } = harness();
    const { rows } = await openRoomList(client);
    const rowOf = (roomId: RoomId): RoomSummary => {
      const row = rows.find((entry) => entry.id === roomId);
      if (!row) throw new Error(`row ${roomId} is not in the list`);
      return row;
    };

    expectResponse(await client.setRoomPinned(DEMO_ROOM.telegramPriya, true), 'RoomFlagsUpdated');
    expect(rowOf(DEMO_ROOM.telegramPriya).pinned).toBe(true);
    expect(rows[0].id).toBe(DEMO_ROOM.telegramDev);
    expect(rows[1].id).toBe(DEMO_ROOM.telegramPriya);

    expectResponse(await client.setRoomArchived(DEMO_ROOM.telegramJonas, true), 'RoomFlagsUpdated');
    expect(rowOf(DEMO_ROOM.telegramJonas).archived).toBe(true);
    expect(rows.at(-1)?.archived).toBe(true);

    expectResponse(await client.setRoomMuted(DEMO_ROOM.messengerLena, 1_900_000_000_000), 'RoomFlagsUpdated');
    expect(rowOf(DEMO_ROOM.messengerLena).muted_until).toBe(1_900_000_000_000);

    expectResponse(await client.setRoomMarkedUnread(DEMO_ROOM.telegramSam, false), 'RoomFlagsUpdated');
    expect(rowOf(DEMO_ROOM.telegramSam).marked_unread).toBe(false);

    expect(rowOf(DEMO_ROOM.telegramJonas).unread_notifications.notification_count).toBeGreaterThan(0);
    expectResponse(await client.markRoomRead(DEMO_ROOM.telegramJonas), 'RoomFlagsUpdated');
    expect(rowOf(DEMO_ROOM.telegramJonas).unread_notifications).toEqual({
      highlight_count: 0,
      notification_count: 0,
    });
  });

  it('keeps a filtered row in sync with its flags', async () => {
    const { client } = harness();
    const { rows } = await openRoomList(client, 'Jonas');

    expect(rows.map((row) => row.name)).toEqual(['Jonas Weber']);
    expectResponse(await client.setRoomPinned(DEMO_ROOM.telegramJonas, true), 'RoomFlagsUpdated');
    expect(rows.map((row) => row.pinned)).toEqual([true]);
  });
});

describe('timeline and messages', () => {
  it('queues a text message, shows the local echo and resolves it to Sent', async () => {
    const { client } = harness();
    const { items } = await openTimeline(client, DEMO_ROOM.telegramJonas);
    const pushed: TimelineItemDto[] = [];
    client.subscribe((event) => {
      if (
        event.type === 'TimelineDiff' &&
        event.payload.room_id === DEMO_ROOM.telegramJonas &&
        event.payload.diff.type === 'PushBack'
      ) {
        pushed.push(event.payload.diff.content.value);
      }
    });

    const queued = expectResponse(
      await client.sendText(DEMO_ROOM.telegramJonas, 'Running the release checklist now.'),
      'MessageQueued'
    );
    const txnId = queued.payload.txn_id;

    await vi.waitFor(() => expect(pushed.some((item) => item.transaction_id === txnId)).toBe(true));
    const echo = pushed.find((item) => item.transaction_id === txnId);
    if (!echo) throw new Error('the local echo never arrived');
    expect(echo.is_own).toBe(true);
    expect(textBody(echo)).toBe('Running the release checklist now.');
    expect(echo.send_state).toEqual({ status: 'Queued', data: { transaction_id: txnId } });

    await vi.waitFor(() =>
      expect(items.find((item) => item.transaction_id === txnId)?.send_state.status).toBe('Sent')
    );
    expect(items.find((item) => item.transaction_id === txnId)?.event_id).toBeTruthy();
  });

  it('edits, reacts to and deletes a sent message through timeline diffs', async () => {
    const { client } = harness();
    const { items } = await openTimeline(client, DEMO_ROOM.telegramJonas);
    const sent = lastMessage(items);
    const eventId = sent.event_id;
    if (!eventId) throw new Error('seeded message has no id');

    expectResponse(await client.editMessage(DEMO_ROOM.telegramJonas, eventId, 'Edited body'), 'MessageEdited');
    const edited = items.find((item) => item.event_id === eventId);
    expect(edited && textBody(edited)).toBe('Edited body');
    expect(edited?.edited).toBe(true);

    expectResponse(await client.toggleReaction(DEMO_ROOM.telegramJonas, eventId, '👍'), 'ReactionToggled');
    const reacted = items.find((item) => item.event_id === eventId);
    expect(reacted?.reactions).toEqual([{ key: '👍', count: 1, senders: ['tg:478200113'], by_me: true }]);

    expectResponse(await client.toggleReaction(DEMO_ROOM.telegramJonas, eventId, '👍'), 'ReactionToggled');
    expect(items.find((item) => item.event_id === eventId)?.reactions).toEqual([]);

    expectResponse(
      await client.deleteMessage(DEMO_ROOM.telegramJonas, eventId, true),
      'MessageDeleted'
    );
    expect(items.find((item) => item.event_id === eventId)?.content.kind).toBe('Deleted');
  });

  it('retries a failed outgoing message and sends it', async () => {
    const { client } = harness();
    const { items } = await openTimeline(client, DEMO_ROOM.messengerFlat);
    const failed = items.find((item) => item.transaction_id === 'txn_demo_failed');
    expect(failed?.send_state.status).toBe('Failed');

    expectResponse(await client.retrySend(DEMO_ROOM.messengerFlat, 'txn_demo_failed'), 'SendRetried');
    await vi.waitFor(() =>
      expect(items.find((item) => item.transaction_id === 'txn_demo_failed')?.send_state.status).toBe('Sent')
    );
  });

  it('cancels a failed outgoing message and removes the row', async () => {
    const { client } = harness();
    const { items } = await openTimeline(client, DEMO_ROOM.messengerFlat);
    const before = items.length;

    expectResponse(await client.cancelSend(DEMO_ROOM.messengerFlat, 'txn_demo_failed'), 'SendCancelled');
    expect(items).toHaveLength(before - 1);
    expect(items.some((item) => item.transaction_id === 'txn_demo_failed')).toBe(false);
  });

  it('forwards a message into another chat and reports unknown targets', async () => {
    const { client } = harness();
    const source = await openTimeline(client, DEMO_ROOM.telegramDev);
    const original = source.items.find((item) => item.content.kind === 'TextMessage');
    const eventId = original?.event_id;
    if (!eventId || !original) throw new Error('no text message to forward');
    const body = textBody(original);
    const target = await openTimeline(client, DEMO_ROOM.messengerTomas);

    const forwarded = expectResponse(
      await client.forwardEvents(DEMO_ROOM.telegramDev, [eventId], [DEMO_ROOM.messengerTomas, 'missing-chat']),
      'EventsForwarded'
    );
    expect(forwarded.payload.failures).toEqual([{ room_id: 'missing-chat', error: 'chat not found' }]);

    await vi.waitFor(() =>
      expect(target.items.some((item) => textBody(item) === body && item.is_own)).toBe(true)
    );
  });

  it('stores and clears a draft, and mirrors it into the room row', async () => {
    const { client } = harness();
    const { rows } = await openRoomList(client);

    expectResponse(await client.saveDraft(DEMO_ROOM.telegramPriya, 'Draft body'), 'DraftSaved');
    const draft = expectResponse(await client.getDraft(DEMO_ROOM.telegramPriya), 'Draft').payload.draft;
    expect(draft).toEqual({ text: 'Draft body', reply_to: null });
    expect(rows.find((row) => row.id === DEMO_ROOM.telegramPriya)?.draft).toBe('Draft body');

    const drafts = expectResponse(await client.listDrafts(), 'Drafts').payload.drafts;
    expect(drafts).toContainEqual({ room_id: DEMO_ROOM.telegramPriya, text: 'Draft body' });

    expectResponse(await client.saveDraft(DEMO_ROOM.telegramPriya, ''), 'DraftSaved');
    expect(expectResponse(await client.getDraft(DEMO_ROOM.telegramPriya), 'Draft').payload.draft).toBeNull();
    expect(rows.find((row) => row.id === DEMO_ROOM.telegramPriya)?.draft).toBeNull();
  });

  it('searches one chat and every chat in the local index', async () => {
    const { client } = harness();

    const roomHits = expectResponse(
      await client.searchRoomMessages(DEMO_ROOM.telegramDev, 'SQLite'),
      'RoomSearchResults'
    ).payload.hits;
    expect(roomHits).toHaveLength(1);
    expect(roomHits[0].body).toContain('SQLite');

    const allHits = expectResponse(await client.searchAllMessages('plumber'), 'AllMessagesFound').payload.hits;
    expect(allHits).toHaveLength(1);
    expect(allHits[0].room_name).toBe('Flat 4B');
    expect(allHits[0].room_id).toBe(DEMO_ROOM.messengerFlat);
  });

  it('lists shared media, files and links with a next-page cursor', async () => {
    const { client } = harness();

    const images = expectResponse(
      await client.getRoomMedia(DEMO_ROOM.messengerLena, 'Media'),
      'RoomMedia'
    ).payload;
    expect(images.items).toHaveLength(1);
    expect(images.items[0].kind).toBe('image');
    expect(images.next).toBeNull();
    const source = images.items[0].source;
    if (!source) throw new Error('image item has no source');

    const media = expectResponse(await client.getMedia(source, 'file'), 'Media');
    expect(media.payload.path.startsWith('data:image/svg+xml')).toBe(true);

    const files = expectResponse(
      await client.getRoomMedia(DEMO_ROOM.telegramDev, 'Files', null, 1),
      'RoomMedia'
    ).payload;
    expect(files.items[0].body).toBe('store-schema-v1.sql');

    const links = expectResponse(
      await client.getRoomMedia(DEMO_ROOM.messengerCycling, 'Links'),
      'RoomMedia'
    ).payload.items;
    expect(links[0].urls).toEqual(['https://example.org/routes/valley-loop']);
  });

  it('keeps a private note per chat', async () => {
    const { client } = harness();

    expect(expectResponse(await client.getRoomNote(DEMO_ROOM.messengerFlat), 'RoomNote').payload.note).toBeNull();
    expectResponse(await client.setRoomNote(DEMO_ROOM.messengerFlat, 'Spare key is with the neighbour.'), 'RoomNoteSet');
    expect(expectResponse(await client.getRoomNote(DEMO_ROOM.messengerFlat), 'RoomNote').payload.note).toBe(
      'Spare key is with the neighbour.'
    );
    const details = expectResponse(await client.getRoomDetails(DEMO_ROOM.messengerFlat), 'RoomDetailsRetrieved');
    expect(details.payload.details.note).toBe('Spare key is with the neighbour.');

    expectResponse(await client.setRoomNote(DEMO_ROOM.messengerFlat, '   '), 'RoomNoteSet');
    expect(expectResponse(await client.getRoomNote(DEMO_ROOM.messengerFlat), 'RoomNote').payload.note).toBeNull();
  });

  it('creates, assigns and deletes a chat tag', async () => {
    const { client } = harness();
    const { rows } = await openRoomList(client);

    const created = expectResponse(await client.createRoomTag('Travel plans', 'amber'), 'RoomTagCreated');
    expect(created.payload.tag).toMatchObject({ id: 'travel-plans', name: 'Travel plans', colour: 'amber' });

    expectResponse(await client.setRoomTag(DEMO_ROOM.telegramSam, 'travel-plans', true), 'RoomFlagsUpdated');
    expect(rows.find((row) => row.id === DEMO_ROOM.telegramSam)?.tags).toContain('travel-plans');

    expectResponse(await client.deleteRoomTag('travel-plans'), 'RoomTagsUpdated');
    const catalogue = expectResponse(await client.getTagCatalog(), 'TagCatalog').payload.tags;
    expect(catalogue.map((tag) => tag.id)).toEqual(['work', 'friends', 'home']);
    expect(rows.find((row) => row.id === DEMO_ROOM.telegramSam)?.tags).not.toContain('travel-plans');
  });

  it('reads members, details and a user profile for a chat', async () => {
    const { client } = harness();

    const members = expectResponse(await client.getRoomMembers(DEMO_ROOM.messengerFlat), 'RoomMembers').payload.members;
    expect(members.map((member) => member.display_name)).toEqual(['Mara Vogel', 'Tomás Rivera', 'Ivy Chen']);

    const details = expectResponse(await client.getRoomDetails(DEMO_ROOM.messengerFlat), 'RoomDetailsRetrieved');
    expect(details.payload.details).toMatchObject({
      name: 'Flat 4B',
      kind: 'group',
      member_count: 3,
      network: 'messenger',
    });

    const profile = expectResponse(
      await client.getUserProfile(DEMO_ACCOUNT.messenger, 'fb:100073211994'),
      'UserProfile'
    ).payload.profile;
    expect(profile).toMatchObject({ display_name: 'Lena Fischer', is_me: false, dm_room_id: DEMO_ROOM.messengerLena });
  });

  it('sends an attachment through upload progress and resolves it to Sent', async () => {
    const { client, events } = harness();
    const { items } = await openTimeline(client, DEMO_ROOM.telegramPriya);

    const queued = expectResponse(
      await client.sendAttachment(DEMO_ROOM.telegramPriya, '/tmp/plan.pdf', 'application/pdf', 'Q4 plan'),
      'MessageQueued'
    );
    const echo = (): TimelineItemDto | undefined =>
      items.find((item) => item.transaction_id === queued.payload.txn_id);
    await vi.waitFor(() => expect(echo()?.send_state.status).toBe('Sent'));
    const attachment = echo();
    if (!attachment) throw new Error('attachment echo is missing');
    expect(attachment.content.kind).toBe('Media');
    if (attachment.content.kind !== 'Media') return;
    expect(attachment.content.data).toMatchObject({ body: 'Q4 plan', media_kind: 'file', mime_type: 'application/pdf' });

    const progress = events.filter((event) => event.type === 'UploadProgress');
    expect(progress.length).toBeGreaterThanOrEqual(2);
    expect(progress.at(-1)).toMatchObject({
      payload: { txn_id: queued.payload.txn_id, sent_bytes: expect.any(Number), total_bytes: expect.any(Number) },
    });
  });
});
