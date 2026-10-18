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
  await page.setViewport({ width: 1280, height: 800 });
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

  step('open room: the other user message is rendered with a coloured name');
  await page.evaluate(() => [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('From Bob')).click());
  await page.waitForFunction(() => document.body.innerText.includes('Hi from bob'), { timeout: 60000 });

  step('send a markdown message: local echo then confirmed, rendered as formatted html');
  await page.click('textarea[aria-label=Message]');
  await page.type('textarea[aria-label=Message]', 'Hello **there**');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => [...document.querySelectorAll('.body strong')].some((e) => e.textContent === 'there'), { timeout: 30000 });
  await page.waitForFunction(() => (() => { const rows = [...document.querySelectorAll('.row.own')]; return rows.length > 0 && rows.every((r) => !r.querySelector('.state, .failed-actions')); })(), { timeout: 30000 });

  step('react to the first message');
  const bobRow = await page.evaluateHandle(() => [...document.querySelectorAll('.row')].find((r) => r.textContent.includes('Hi from bob')));
  await bobRow.asElement().hover();
  await page.evaluate((row) => row.querySelector('button[aria-label=React]').click(), bobRow);
  await page.waitForSelector('.picker button', { visible: true });
  if (process.env.E2E_TRACE) console.log('picker top element:', await page.evaluate(() => { const b = document.querySelector('.picker button'); const r = b.getBoundingClientRect(); const t = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return t && (t.className + ' / ' + t.tagName + ' / ' + t.outerHTML.slice(0, 120)); }));
  await page.evaluate(() => document.querySelector('.picker button').click());
  await page.waitForFunction(() => document.querySelector('.chip.mine'), { timeout: 30000 });

  step('hover an icon button shows a tooltip');
  await page.hover('button[aria-label="Search"]');
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
  await page.evaluate(() => [...document.querySelectorAll('section[aria-label="Room info"] [role=tab]')].find((t) => t.textContent.includes('Media')).click());
  await new Promise((r) => setTimeout(r, 1200));
  await uxShot(page, 'room-info-media');
  await page.click('button[aria-label="Close room info"]');
  await page.waitForFunction(() => !document.querySelector('section[aria-label="Room info"]'), { timeout: 5000 });

  step('right-click a room: rename is disabled without the power level');
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('From Bob'));
    const r = row.getBoundingClientRect();
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10 }));
  });
  await page.waitForSelector('[role=menu] [role=menuitem]', { visible: true, timeout: 5000 });
  await page.waitForFunction(() => [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Rename room'))?.getAttribute('aria-disabled') === 'true', { timeout: 10000 });
  await uxShot(page, 'context-menu');
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
  await page.waitForFunction(() => [...document.querySelectorAll('button[aria-label^="Room info for"]')].some((h) => h.getAttribute('aria-label') === 'Room info for Created in UI') && document.body.innerText.includes('End-to-end encrypted'), { timeout: 60000 });
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

  step('browse the public room directory');
  await page.click('button[aria-label="Browse public rooms"]');
  await page.waitForSelector('input[aria-label="Search public rooms"]', { visible: true });
  await page.type('input[aria-label="Search public rooms"]', dirName);
  await page.waitForFunction((name) => document.querySelector('.results li')?.innerText.includes(name), { timeout: 30000 }, dirName);
  await page.click('button[aria-label="Close"]');
  await page.waitForFunction(() => !document.querySelector('input[aria-label="Search public rooms"]'), { timeout: 10000 });

  step('settings open and sign out returns to onboarding');
  await page.click('button[aria-label="Settings"]');
  await new Promise((r) => setTimeout(r, 600));
  await uxShot(page, 'settings');
  await page.waitForSelector('[role=dialog]');
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
