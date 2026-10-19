#!/usr/bin/env node
/**
 * Full-stack smoke test: real UI (built) + real engine (alloy-bridge over WebSocket) + real Synapse.
 *
 *   ALLOY_TEST_HS=http://localhost:8008 ALLOY_BRIDGE_BIN=/path/to/alloy-bridge node scripts/e2e-bridge.mjs
 *
 * Needs the Synapse from tests/synapse and Chrome. Exits non-zero on any failed expectation.
 */
import puppeteer from 'puppeteer-core';
import http from 'node:http';
import fs from 'node:fs';
import { randomBytes } from 'node:crypto';
import os from 'node:os';
import { spawn } from 'node:child_process';
import { resolve, dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const build = resolve(__dirname, '../build');
const shots = resolve(__dirname, '../../../../docs/spikes/ui');
const HS = process.env.ALLOY_TEST_HS ?? 'http://localhost:8008';
const BRIDGE_BIN = process.env.ALLOY_BRIDGE_BIN;
const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const BRIDGE_PORT = 8766;
const UI_PORT = 5176;
const PW = 'testpassword123!';
fs.mkdirSync(shots, { recursive: true });

if (!BRIDGE_BIN) throw new Error('set ALLOY_BRIDGE_BIN to the alloy-bridge binary');

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const api = async (path, body, token) => {
  const res = await fetch(`${HS}/_matrix/client/v3${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...(token ? { authorization: `Bearer ${token}` } : {}) },
    body: JSON.stringify(body),
  });
  const json = await res.json();
  if (!res.ok) throw new Error(`${path}: ${JSON.stringify(json)}`);
  return json;
};
const register = async (username) => {
  const r = await api('/register', { username, password: PW, auth: { type: 'm.login.dummy' } });
  return { token: r.access_token, id: r.user_id };
};

// --- fixtures: alice-like user U, and V who invites U to a room and says hi
const stamp = Date.now().toString(36);
const uName = `e2eu${stamp}`;
const vName = `e2ev${stamp}`;
const u = await register(uName);
const v = await register(vName);
const uxShot = async (page, name) => { if (process.env.E2E_UX_SHOTS) await page.screenshot({ path: `${process.env.E2E_UX_SHOTS}/${name}.png` }); };
const room = await api('/createRoom', { name: 'From Bob', invite: [u.id] }, v.token);
await api(`/join/${encodeURIComponent(room.room_id)}`, {}, u.token);
await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(room.room_id)}/send/m.room.message/t1`, {
  method: 'PUT',
  headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
  body: JSON.stringify({ msgtype: 'm.text', body: 'Hi from bob' }),
});

const dirName = `dir-e2e-${Date.now()}`;
await api('/createRoom', { name: dirName, visibility: 'public', preset: 'public_chat' }, v.token);
const invited = await api('/createRoom', { name: 'Invite me', invite: [u.id] }, v.token);
// A room with far more history than one screen: V writes 80 numbered messages, U has not read them.
const longRoom = await api('/createRoom', { name: 'Long History', invite: [u.id] }, v.token);
await api(`/join/${encodeURIComponent(longRoom.room_id)}`, {}, u.token);
for (let i = 1; i <= 80; i++) {
  await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(longRoom.room_id)}/send/m.room.message/h${i}`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
    body: JSON.stringify({ msgtype: 'm.text', body: `history ${String(i).padStart(2, '0')}` }),
  });
}

// --- services: bridge + static UI server
const dataDir = fs.mkdtempSync(join(os.tmpdir(), 'alloy-e2e-'));
const BRIDGE_TOKEN = randomBytes(24).toString('hex');
const bridge = spawn(BRIDGE_BIN, ['--port', String(BRIDGE_PORT), '--data-dir', dataDir], {
  stdio: ['ignore', 'pipe', 'inherit'],
  env: { ...process.env, ALLOY_BRIDGE_TOKEN: BRIDGE_TOKEN },
});
await new Promise((ok, fail) => {
  bridge.stdout.on('data', (d) => String(d).includes('listening') && ok());
  bridge.on('exit', (c) => fail(new Error(`bridge exited ${c}`)));
});

const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = http.createServer((req, res) => {
  let p = resolve(build, '.' + decodeURIComponent(req.url.split('?')[0]));
  if (!p.startsWith(build) || !fs.existsSync(p) || fs.statSync(p).isDirectory()) p = resolve(build, 'index.html');
  res.writeHead(200, { 'content-type': types[extname(p)] ?? 'application/octet-stream' });
  fs.createReadStream(p).pipe(res);
});
await new Promise((r) => server.listen(UI_PORT, r));

const browser = await puppeteer.launch({ executablePath: CHROME, headless: 'new', args: ['--no-sandbox'] });
let failed = null;
const step = (n) => console.log(`- ${n}`);
try {
  const page = await browser.newPage();
  const MOBILE = process.env.E2E_MOBILE === '1';
  await page.setViewport(MOBILE ? { width: 390, height: 844, deviceScaleFactor: 2, isMobile: true, hasTouch: true } : { width: 1280, height: 800 });
  page.on('pageerror', (e) => console.error('pageerror:', e.message));
  page.on('console', (m) => ['error', 'warning', 'log'].includes(m.type()) && console.error('console:', m.text()));
  if (process.env.E2E_TRACE) {
    const cdp = await page.createCDPSession();
    await cdp.send('Network.enable');
    cdp.on('Network.webSocketFrameSent', (e) => console.log('>>', e.response.payloadData.slice(0, 300)));
    cdp.on('Network.webSocketFrameReceived', (e) => { const d = e.response.payloadData; if (!d.includes('RoomListDiff')) console.log('<<', d.slice(0, 600)); });
  }
  await page.goto(`http://localhost:${UI_PORT}/?engine=ws://127.0.0.1:${BRIDGE_PORT}&token=${BRIDGE_TOKEN}`, { waitUntil: 'networkidle0' });

  step('onboarding: discover server, sign in');
  await page.waitForSelector('input[placeholder*="matrix.org"]');
  await page.$eval('input[placeholder*="matrix.org"]', (el) => (el.value = ''));
  await page.type('input[placeholder*="matrix.org"]', HS);
  await page.click('button[type=submit]');
  await page.waitForSelector('input[autocomplete=username]', { timeout: 20000 });
  await page.type('input[autocomplete=username]', uName);
  await page.type('input[type=password]', PW);
  await page.click('button[type=submit]');

  step('room list streams in with the invited room');
  await page.waitForFunction(() => [...document.querySelectorAll('button.row .title')].some((e) => e.textContent === 'From Bob'), { timeout: 60000 });
  await page.screenshot({ path: resolve(shots, 'e2e-1-room-list.png') });

  if (MOBILE) {
    const problems = [];
    const audit = async (name) => {
      const r = await page.evaluate(() => {
        const se = document.scrollingElement;
        const vis = (el) => { const b = el.getBoundingClientRect(); const cs = getComputedStyle(el); return b.width > 0 && b.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none' && b.bottom > 0 && b.top < innerHeight; };
        const small = [...document.querySelectorAll('button, a[href], [role=button], [role=tab], [role=menuitem], input, select, textarea')]
          .filter(vis).map((el) => ({ el, b: el.getBoundingClientRect() })).filter(({ el, b }) => { if (b.width >= 44 && b.height >= 44) return false; if (el.getAttribute('role') === 'switch') { const pb = getComputedStyle(el, '::before'); return !(parseFloat(pb.height) >= 44); } return true; })
          .map(({ el, b }) => `${el.tagName.toLowerCase()}[${el.getAttribute('aria-label') || el.textContent.trim().slice(0, 16)}] ${Math.round(b.width)}x${Math.round(b.height)}`);
        const wide = [...document.querySelectorAll('body *')].filter((el) => vis(el) && el.getBoundingClientRect().right > innerWidth + 1).slice(0, 5).map((el) => `${el.tagName.toLowerCase()}.${String(el.className).slice(0, 30)}`);
        window.scrollTo(0, 200); const moved = window.scrollY; window.scrollTo(0, 0);
        return { pageMoves: moved, shellScroll: se.scrollHeight - innerHeight, hScroll: se.scrollWidth - innerWidth, bodyScrollable: getComputedStyle(document.body).overflow, small, wide, vv: window.visualViewport ? Math.round(window.visualViewport.height) : null };
      });
      console.log(`AUDIT ${name}: pageMoves=${r.pageMoves} shellScroll=${r.shellScroll} hScroll=${r.hScroll} small=${r.small.length} wide=${r.wide.join(',') || '-'}`);
      problems.push(...(r.pageMoves > 0 ? [`${name}: page scrolls`] : []), ...(r.hScroll > 0 ? [`${name}: horizontal overflow`] : []), ...(r.wide.length ? [`${name}: overflow ${r.wide[0]}`] : []), ...r.small.map((t) => `${name}: small ${t}`));
      for (const t of r.small.slice(0, 12)) console.log(`   small: ${t}`);
      await uxShot(page, `m-${name}`);
    };
    const tapSel = async (sel) => { await page.waitForSelector(sel, { visible: true, timeout: 15000 }); await page.tap(sel); await new Promise((r) => setTimeout(r, 700)); };
    const settle = () => new Promise((r) => setTimeout(r, 700));
    step('mobile: room list');
    await audit('1-room-list');
    step('mobile: open a chat');
    await page.evaluate(() => [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('From Bob')).click());
    await page.waitForFunction(() => document.body.innerText.includes('Hi from bob'), { timeout: 60000 });
    await settle();
    await audit('2-chat');
    step('mobile: composer focus');
    await page.tap('textarea[aria-label^=Message]'); await settle();
    await page.type('textarea[aria-label^=Message]', 'phone message with a fairly long text to see wrapping behaviour in the composer area');
    await audit('3-composer');
    await page.keyboard.press('Enter'); await settle();
    step('mobile: room info');
    await tapSel('button[aria-label^="Room info for"]');
    await audit('4-room-info');
    await page.evaluate(() => document.querySelector('button[aria-label="Close room info"]')?.click()); await settle();
    step('mobile: room info sub-views');
    await tapSel('button[aria-label^="Room info for"]');
    await page.evaluate(() => [...document.querySelectorAll('section[aria-label="Room info"] button')].find((b) => /^View all/.test(b.textContent.trim())).click()); await settle();
    await audit('5-room-info-members');
    await page.evaluate(() => document.querySelector('button[aria-label="Back to details"]')?.click()); await settle();
    await page.evaluate(() => document.querySelector('button[aria-label="Close room info"], button[aria-label="Back"]')?.click()); await settle();
    step('mobile: back to list, new room, directory, settings');
    await page.evaluate(() => document.querySelector('button[aria-label="Back to rooms"]')?.click()); await settle();
    await tapSel('button[aria-label="New room"]'); await audit('6-new-room'); await page.keyboard.press('Escape'); await settle();
    await page.evaluate(() => [...document.querySelectorAll('nav[aria-label=Primary] button')].find((b) => b.textContent.includes('Discover')).click()); await settle(); await audit('7-directory'); await page.keyboard.press('Escape'); await settle();
    await page.evaluate(() => [...document.querySelectorAll('nav[aria-label=Primary] button')].find((b) => b.textContent.includes('Settings')).click()); await settle(); await audit('8-settings-nav');
    await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.trim() === 'Appearance').click()); await settle();
    await audit('9-settings-appearance');
    await page.evaluate(() => document.querySelector('button[aria-label="Back to settings"]').click()); await settle();
    await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.trim() === 'Help & About').click()); await settle();
    await audit('10-settings-about');
    if (problems.length) { console.log('MOBILE PROBLEMS (' + problems.length + '):\n' + [...new Set(problems)].join('\n')); }
    console.log(problems.length ? 'E2E MOBILE AUDIT FAILED' : 'E2E OK (mobile audit)');
    await browser.close(); server.close(); bridge.kill(); fs.rmSync(dataDir, { recursive: true, force: true });
    process.exit(problems.length && process.env.E2E_STRICT ? 1 : 0);
  }

  step('open room: the other user message is rendered with a coloured name');
  await page.evaluate(() => [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('From Bob')).click());
  await page.waitForFunction(() => document.body.innerText.includes('Hi from bob'), { timeout: 60000 });

  step('search in the open room: find a message by its text, jump to it, close with Escape');
  await page.click('button[aria-label="Search in room"]');
  await page.waitForSelector('aside[aria-label="Search in room"]', { visible: true, timeout: 15000 });
  await page.type('aside[aria-label="Search in room"] input[aria-label="Search messages"]', 'from bob');
  await page.waitForFunction(() => [...document.querySelectorAll('aside[aria-label="Search in room"] button.hit')].some((b) => b.textContent.includes('Hi from bob')), { timeout: 30000 });
  await page.evaluate(() => [...document.querySelectorAll('aside[aria-label="Search in room"] button.hit')].find((b) => b.textContent.includes('Hi from bob')).click());
  await new Promise((r) => setTimeout(r, 800));
  if (!(await page.$('aside[aria-label="Search in room"]'))) throw new Error('search panel closed after a desktop jump');
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('aside[aria-label="Search in room"]'), { timeout: 5000 });

  step('send a markdown message: local echo then confirmed, rendered as formatted html');
  await page.click('textarea[aria-label=Message]');
  await page.type('textarea[aria-label=Message]', 'Hello **there**');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => [...document.querySelectorAll('.body strong')].some((e) => e.textContent === 'there'), { timeout: 30000 });
  await page.waitForFunction(() => (() => { const rows = [...document.querySelectorAll('.row.own')]; return rows.length > 0 && rows.every((r) => !r.querySelector('.state, .failed-actions')); })(), { timeout: 30000 });

  step('send a link in an unencrypted room: the message renders, and any preview card is well-formed');
  await page.type('textarea[aria-label=Message]', 'See https://example.com/alloy-e2e');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => [...document.querySelectorAll('.body')].some((e) => e.textContent.includes('alloy-e2e')), { timeout: 30000 });
  await new Promise((r) => setTimeout(r, 2000));
  if (await page.$('aside.link-preview')) {
    const label = await page.$eval('aside.link-preview button.open', (b) => b.textContent.trim());
    if (!label) throw new Error('link preview card rendered without any text');
  }

  step('react to the first message');
  const bobRow = await page.evaluateHandle(() => [...document.querySelectorAll('.row')].find((r) => r.textContent.includes('Hi from bob')));
  await bobRow.asElement().hover();
  await page.evaluate((row) => row.querySelector('button[aria-label=React]').click(), bobRow);
  // The shared emoji picker: search narrows the grid, picking an emoji toggles the reaction.
  await page.waitForSelector('.emoji-picker .cell', { visible: true, timeout: 30000 });
  await uxShot(page, 'emoji-picker');
  await page.type('.emoji-picker input[aria-label="Search emoji"]', 'thumbs up');
  await page.waitForFunction(() => document.querySelector('.emoji-picker .heading')?.textContent === 'Results' && /thumbs up/i.test(document.querySelector('.emoji-picker .cell')?.getAttribute('aria-label') ?? ''), { timeout: 10000 });
  await page.evaluate(() => document.querySelector('.emoji-picker .cell').click());
  await page.waitForFunction(() => document.querySelector('.chip.mine'), { timeout: 30000 });

  step('hover an icon button shows a tooltip');
  await page.hover('button[aria-label="Search in room"]');
  await page.waitForSelector('[role=tooltip]', { visible: true, timeout: 5000 });
  await page.waitForFunction(() => document.querySelector('[role=tooltip]')?.textContent.includes('Search'), { timeout: 5000 });
  await page.mouse.move(5, 5);

  step('clicking a sender name opens the profile card');
  await page.evaluate(() => [...document.querySelectorAll('.row button.alloy-user-name')].find((b) => !b.closest('.own')).click());
  await page.waitForSelector('[role=dialog][aria-label="User profile"]', { visible: true, timeout: 15000 });
  await page.waitForFunction(() => document.querySelector('[role=dialog][aria-label="User profile"]').innerText.includes('@'), { timeout: 15000 });
  await uxShot(page, 'profile-card');
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('[role=dialog][aria-label="User profile"]'), { timeout: 5000 });

  step('room info panel: rename, topic and a private note');
  await page.click('button[aria-label^="Room info for"]');
  await page.waitForSelector('section[aria-label="Room info"]', { visible: true, timeout: 15000 });
  await page.waitForSelector('section[aria-label="Room info"] textarea', { visible: true, timeout: 15000 });
  await page.evaluate(() => {
    const areas = [...document.querySelectorAll('section[aria-label="Room info"] textarea')];
    const note = areas[areas.length - 1];
    note.focus();
    note.value = 'remember: e2e note';
    note.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await page.waitForFunction(() => [...document.querySelectorAll('section[aria-label="Room info"] [role=status]')].some((e) => e.textContent.includes('Saved')), { timeout: 15000 });
  await uxShot(page, 'room-info');
  // Members: the overview lists a few, 'View all' drills into the full list and Back returns.
  await page.evaluate(() => [...document.querySelectorAll('section[aria-label="Room info"] button')].find((b) => /^View all/.test(b.textContent.trim())).click());
  await new Promise((r) => setTimeout(r, 800));
  await uxShot(page, 'room-info-members');
  await page.click('section[aria-label="Room info"] button[aria-label="Back to details"]');
  await page.click('button[aria-label="Close room info"]');
  await page.waitForFunction(() => !document.querySelector('section[aria-label="Room info"]'), { timeout: 5000 });

  step('right-click a room: rename is disabled without the power level');
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('From Bob'));
    const r = row.getBoundingClientRect();
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10 }));
  });
  await page.waitForSelector('[role=menu] [role=menuitem]', { visible: true, timeout: 5000 });
  // Tiered menu: there is no 'Open' entry; Room settings opens a flyout that holds Rename room.
  const topLabels = await page.$$eval('[role=menu] [role=menuitem]', (els) => els.map((e) => e.textContent.trim()));
  if (topLabels.some((l) => l === 'Open')) throw new Error(`room menu still has an Open entry: ${topLabels}`);
  await uxShot(page, 'context-menu');
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Room settings')).click());
  await page.waitForFunction(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Rename room'))?.getAttribute('aria-disabled') === 'true', { timeout: 10000 });
  await uxShot(page, 'context-menu-flyout');
  await page.keyboard.press('Escape');

  step('reply in a thread from the panel');
  await bobRow.asElement().hover();
  await page.evaluate((row) => row.querySelector('button[aria-label="Reply in thread"]').click(), bobRow);
  await page.waitForSelector('aside[aria-label=Thread] textarea[aria-label=Message]', { visible: true, timeout: 30000 });
  await page.type('aside[aria-label=Thread] textarea[aria-label=Message]', 'in the thread');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('aside[aria-label=Thread]')?.innerText.includes('in the thread'), { timeout: 30000 });
  await page.click('aside[aria-label=Thread] button[aria-label="Close thread"]');
  await page.waitForFunction(() => !document.querySelector('aside[aria-label=Thread]'), { timeout: 10000 });

  step('reply to a message: the quote shows the full sender name above the reply text');
  await bobRow.asElement().hover();
  await page.evaluate((row) => row.querySelector('button[aria-label="Reply"]').click(), bobRow);
  await page.click('textarea[aria-label=Message]');
  await page.type('textarea[aria-label=Message]', 'a quoted reply');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => !!document.querySelector('.reply-quote'), { timeout: 30000 });
  const quoteText = await page.$eval('.reply-quote', (q) => q.innerText);
  if (!quoteText.includes('e2ev') || quoteText.includes('…') && quoteText.split('\n')[0].length < 6) throw new Error(`reply quote is cramped or truncated: ${quoteText}`);
  await uxShot(page, 'reply-quote');

  step('edit then delete own message');
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('.row.own')].find((r) => r.textContent.includes('Hello'));
    row.querySelector('button[aria-label=Edit]').click();
  });
  await page.waitForFunction(() => document.querySelector('textarea[aria-label=Message]').value.includes('Hello'));
  await page.$eval('textarea[aria-label=Message]', (el) => { el.value = ''; el.dispatchEvent(new Event('input', { bubbles: true })); });
  await page.type('textarea[aria-label=Message]', 'Hello edited');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.body.innerText.includes('edited') && document.body.innerText.includes('Hello edited'), { timeout: 30000 });
  await page.screenshot({ path: resolve(shots, 'e2e-2-conversation.png') });
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('.row.own')].find((r) => r.textContent.includes('Hello edited'));
    row.querySelector('button[aria-label=Delete]').click();
  });
  await page.waitForFunction(() => document.body.innerText.includes('Message deleted'), { timeout: 30000 });

  step('invitation: accept it from the room list');
  await page.waitForFunction(() => [...document.querySelectorAll('.row.invite')].some((e) => e.textContent.includes('Invite me')), { timeout: 30000 });
  await page.evaluate(() => [...document.querySelectorAll('.row.invite')].find((e) => e.textContent.includes('Invite me')).querySelector('button').click());
  await page.waitForFunction(() => [...document.querySelectorAll('button.row')].some((e) => e.textContent.includes('Invite me')), { timeout: 30000 });

  step('create an encrypted room from the UI and send a message in it');
  await page.click('button[aria-label="New room"]');
  await page.waitForSelector('[role=dialog] input[type=text]');
  await page.type('[role=dialog] input[type=text]', 'Created in UI');
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.trim() === 'Create').click());
  await page.waitForFunction(() => [...document.querySelectorAll('button[aria-label^="Room info for"]')].some((h) => h.getAttribute('aria-label') === 'Room info for Created in UI') && !!document.querySelector('section.timeline[aria-label*="end-to-end encrypted"]'), { timeout: 60000 });
  await page.click('textarea[aria-label=Message]');
  await page.type('textarea[aria-label=Message]', 'secret hello');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => (() => { const rows = [...document.querySelectorAll('.row.own')]; return rows.length > 0 && rows.every((r) => !r.querySelector('.state, .failed-actions')); })(), { timeout: 60000 });
  await page.screenshot({ path: resolve(shots, 'e2e-3-encrypted-room.png') });

  step('right-click a room you own: rename it from the context menu');
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Created in UI'));
    const r = row.getBoundingClientRect();
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10 }));
  });
  await page.waitForSelector('[role=menu] [role=menuitem]', { visible: true, timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Room settings')).click());
  await page.waitForFunction(() => [...document.querySelectorAll('[role=menuitem]')].some((e) => e.textContent.includes('Rename room')), { timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Rename room')).click());
  await page.waitForSelector('[role=dialog] input', { visible: true, timeout: 5000 });
  await page.click('[role=dialog] input');
  await page.$eval('[role=dialog] input', (el) => el.select());
  await page.keyboard.type('Renamed by E2E');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => [...document.querySelectorAll('button.row .title')].some((e) => e.textContent === 'Renamed by E2E'), { timeout: 30000 });

  step('the sidebar can be resized with the keyboard');
  const handle = '[role=separator][aria-label="Resize sidebar"]';
  const widthOf = () => page.$eval(handle, (el) => ({ now: Number(el.getAttribute('aria-valuenow')), left: el.getBoundingClientRect().left }));
  const before = await widthOf();
  await page.focus(handle);
  for (let i = 0; i < 4; i++) await page.keyboard.press('ArrowRight');
  const after = await widthOf();
  if (!(after.now > before.now && after.left > before.left)) throw new Error(`sidebar did not grow: ${JSON.stringify(before)} -> ${JSON.stringify(after)}`);

  step('a room with a long history: unread badge clears once read, older messages load on scroll');
  await page.waitForFunction(() => [...document.querySelectorAll('button.row.unread')].some((e) => e.textContent.includes('Long History')), { timeout: 60000 });
  // A tall window shows far more than the room opens with: the missing history must load by itself, with no scrolling.
  const viewport = page.viewport();
  await page.setViewport({ ...viewport, height: 6000 });
  await page.evaluate(() => [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Long History')).click());
  await page.waitForFunction(() => document.body.innerText.includes('history 80'), { timeout: 60000 });
  await page.waitForFunction(() => document.body.innerText.includes('history 01'), { timeout: 30000 });
  await page.setViewport(viewport);
  await page.waitForFunction(() => ![...document.querySelectorAll('button.row.unread')].some((e) => e.textContent.includes('Long History')), { timeout: 30000 });
  let reachedOldest = false;
  for (let i = 0; i < 60 && !reachedOldest; i++) {
    await page.evaluate(() => { const v = document.querySelector('section.timeline .viewport > div'); if (v) v.scrollTop = 0; });
    await new Promise((r) => setTimeout(r, 400));
    reachedOldest = await page.evaluate(() => document.body.innerText.includes('history 01'));
  }
  if (!reachedOldest) throw new Error('scrolling up never loaded the oldest message (history 01)');

  step('browse the public room directory');
  await page.click('button[aria-label="Browse public rooms"]');
  await page.waitForSelector('input[aria-label="Search public rooms"]', { visible: true });
  await page.type('input[aria-label="Search public rooms"]', dirName);
  await page.waitForFunction((name) => document.querySelector('.results li')?.innerText.includes(name), { timeout: 30000 }, dirName);
  await page.click('button[aria-label="Close"]');
  await page.waitForFunction(() => !document.querySelector('input[aria-label="Search public rooms"]'), { timeout: 10000 });

  step('tags: create one in settings, assign it from the room menu, filter by it, delete it');
  const openTagsSettings = async () => {
    await page.click('button[aria-label="Settings"]');
    await page.waitForSelector('[role=dialog]');
    await page.evaluate(() => [...document.querySelectorAll('[role=dialog] nav button, [role=dialog] button')].find((b) => b.textContent.trim() === 'Tags').click());
    await page.waitForFunction(() => document.querySelector('[role=dialog]').innerText.includes('New tag'), { timeout: 15000 });
  };
  await openTagsSettings();
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.includes('New tag')).click());
  await page.waitForSelector('[role=dialog][aria-label="New tag"] input', { visible: true, timeout: 5000 });
  await page.type('[role=dialog][aria-label="New tag"] input', 'Work');
  await page.click('[role=dialog][aria-label="New tag"] button[aria-label="blue"]');
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog][aria-label="New tag"] button')].find((b) => b.textContent.trim() === 'Save').click());
  await page.waitForFunction(() => document.querySelector('[role=dialog]').innerText.includes('Work') && !document.querySelector('[role=dialog][aria-label="New tag"]'), { timeout: 15000 });
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('[role=dialog]'), { timeout: 10000 });
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Renamed by E2E'));
    const r = row.getBoundingClientRect();
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10 }));
  });
  // Tags is its own flyout: toggle an existing tag, then create a new one by typing a name and pressing Enter.
  const roomMenu = async () => {
    await page.evaluate(() => {
      const row = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Renamed by E2E'));
      const r = row.getBoundingClientRect();
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10 }));
    });
    await page.waitForSelector('[role=menu] [role=menuitem]', { visible: true, timeout: 5000 });
    await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.trim().startsWith('Tags')).click());
    await page.waitForSelector('input[placeholder*="create a tag"]', { visible: true, timeout: 5000 });
  };
  await page.keyboard.press('Escape');
  await roomMenu();
  await uxShot(page, 'context-menu-tags');
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitemradio]')].find((e) => e.textContent.includes('Work')).click());
  await page.waitForFunction(() => [...document.querySelectorAll('[role=toolbar][aria-label="Filter rooms by tag"] button')].some((b) => b.textContent.includes('Work')), { timeout: 15000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=toolbar][aria-label="Filter rooms by tag"] button')].find((b) => b.textContent.includes('Work')).click());
  await page.waitForFunction(() => { const rows = [...document.querySelectorAll('button.row')]; return rows.length > 0 && rows.every((r) => r.textContent.includes('Renamed by E2E')); }, { timeout: 15000 });
  // Create a tag straight from the room menu's search field.
  await page.keyboard.press('Escape');
  await roomMenu();
  await page.type('input[placeholder*="create a tag"]', 'Urgent');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => [...document.querySelectorAll('[role=toolbar][aria-label="Filter rooms by tag"] button')].some((b) => b.textContent.includes('Urgent')), { timeout: 15000 });
  // Right-click a tag chip: its own menu offers to delete it.
  await page.evaluate(() => {
    const chip = [...document.querySelectorAll('[role=toolbar][aria-label="Filter rooms by tag"] button')].find((b) => b.textContent.includes('Urgent'));
    const r = chip.getBoundingClientRect();
    chip.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 5, clientY: r.top + 5 }));
  });
  await page.waitForFunction(() => [...document.querySelectorAll('[role=menuitem]')].some((e) => e.textContent.includes('Delete tag')), { timeout: 5000 });
  await uxShot(page, 'context-menu-tag-chip');
  // 'Manage tags...' opens Settings directly on the Tags page.
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Manage tags')).click());
  await page.waitForFunction(() => document.querySelector('[role=dialog]')?.innerText.includes('New tag'), { timeout: 10000 });
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('[role=dialog]'), { timeout: 10000 });
  await page.evaluate(() => {
    const chip = [...document.querySelectorAll('[role=toolbar][aria-label="Filter rooms by tag"] button')].find((b) => b.textContent.includes('Urgent'));
    const r = chip.getBoundingClientRect();
    chip.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 5, clientY: r.top + 5 }));
  });
  await page.waitForFunction(() => [...document.querySelectorAll('[role=menuitem]')].some((e) => e.textContent.includes('Delete tag')), { timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Delete tag')).click());
  await page.waitForSelector('[role=dialog]', { visible: true, timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.trim() === 'Delete tag').click());
  await page.waitForFunction(() => ![...document.querySelectorAll('[role=toolbar][aria-label="Filter rooms by tag"] button')].some((b) => b.textContent.includes('Urgent')), { timeout: 15000 });
  await openTagsSettings();
  await page.click('[role=dialog] button[aria-label="Delete Work"]');
  await page.waitForSelector('[role=dialog][aria-label="Delete tag"]', { visible: true, timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.trim() === 'Delete').click());
  await page.waitForFunction(() => ![...document.querySelectorAll('[role=toolbar][aria-label="Filter rooms by tag"] button')].some((b) => b.textContent.includes('Work')), { timeout: 15000 });
  await page.waitForFunction(() => [...document.querySelectorAll('button.row')].some((r) => !r.textContent.includes('Renamed by E2E')), { timeout: 15000 });
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('[role=dialog]'), { timeout: 10000 });

  step('settings open and sign out returns to onboarding');
  await page.click('button[aria-label="Settings"]');
  await new Promise((r) => setTimeout(r, 600));
  await uxShot(page, 'settings');
  await page.waitForSelector('[role=dialog]');

  step('settings change real behaviour: message layout, font size, join/leave events');
  const openPage = async (label) => {
    await page.evaluate((l) => [...document.querySelectorAll('[role=dialog] nav button, [role=dialog] button')].find((b) => b.textContent.trim() === l).click(), label);
    await new Promise((r) => setTimeout(r, 500));
  };
  await openPage('Help & About');
  await uxShot(page, 'settings-about');
  await openPage('Appearance');
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog] label')].find((l) => l.textContent.includes('Bubbles')).click());
  await page.waitForFunction(() => document.documentElement.getAttribute('data-message-layout') === 'bubbles', { timeout: 5000 });
  // Hovering must not resize a bubble (the time used to appear only on hover and push every message down).
  const bubbleTimeShown = await page.evaluate(() => { const t = document.querySelector('.row[data-layout="bubbles"] .bubble-time'); return !!t && getComputedStyle(t).display !== 'none'; });
  if (!bubbleTimeShown) throw new Error('bubble time is hidden until hover: hovering would shift the layout');
  // Our own Select: open the 'Font size' trigger, then pick the 120% item from the popup menu.
  await page.evaluate(() => document.querySelector('[role=dialog] button[aria-label^="Font size"]').click());
  await page.waitForSelector('[role=menuitemradio]', { visible: true, timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitemradio]')].find((i) => i.textContent.includes('120')).click());
  await page.waitForFunction(() => document.documentElement.style.fontSize === '120%', { timeout: 5000 });
  await uxShot(page, 'settings-appearance');
  await openPage('Preferences');
  await uxShot(page, 'settings-preferences');
  await openPage('Notifications');
  await page.waitForFunction(() => document.querySelector('[role=dialog]').innerText.includes('Direct chats'), { timeout: 15000 });
  await uxShot(page, 'settings-notifications');
  await openPage('Devices');
  await page.waitForFunction(() => document.querySelector('[role=dialog]').innerText.includes('This device'), { timeout: 15000 });
  await uxShot(page, 'settings-devices');
  await openPage('Encryption');
  await page.waitForFunction(() => document.querySelector('[role=dialog]').innerText.includes('Session'), { timeout: 15000 });
  await openPage('Keyboard');
  await uxShot(page, 'settings-keyboard');
  await openPage('Account');
  await page.waitForFunction(() => document.querySelector('[role=dialog] input')?.value?.length > 0, { timeout: 15000 });
  await uxShot(page, 'settings-account');
  await page.evaluate(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Sign out').click());
  await page.waitForSelector('input[placeholder*="matrix.org"]', { timeout: 30000 });
  console.log('E2E OK');
} catch (e) {
  failed = e;
  console.error('E2E FAILED:', e.message, (e.stack || '').split('\n').slice(1, 4).join(' | '));
  try {
    const pages = await browser.pages();
    await pages[pages.length - 1].screenshot({ path: resolve(shots, 'e2e-failure.png') });
  } catch {}
} finally {
  await browser.close();
  server.close();
  bridge.kill();
  fs.rmSync(dataDir, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
