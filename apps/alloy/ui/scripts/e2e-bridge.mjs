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
const pinRoom = await api('/createRoom', { name: 'Pin Room', invite: [u.id], power_level_content_override: { users: { [u.id]: 100 } } }, v.token);
await api(`/join/${encodeURIComponent(pinRoom.room_id)}`, {}, u.token);
await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(pinRoom.room_id)}/send/m.room.message/p1`, {
  method: 'PUT',
  headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
  body: JSON.stringify({ msgtype: 'm.text', body: 'pin target alloy-pin' }),
});
// Three pictures sent in a row without captions: the timeline folds them into one album.
const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64');
const upload = await fetch(`${HS}/_matrix/media/v3/upload?filename=a.png`, { method: 'POST', headers: { 'content-type': 'image/png', authorization: `Bearer ${v.token}` }, body: png });
if (!upload.ok) throw new Error(`album fixture: upload failed with ${upload.status} ${await upload.text()}`);
const mxc = (await upload.json()).content_uri;
for (const n of [1, 2, 3]) {
  const sent = await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(pinRoom.room_id)}/send/m.room.message/img${n}`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
    body: JSON.stringify({ msgtype: 'm.image', body: `IMG_000${n}.png`, url: mxc, info: { mimetype: 'image/png', w: 1, h: 1, size: png.length } }),
  });
  if (!sent.ok) throw new Error(`album fixture: sending picture ${n} failed with ${sent.status} ${await sent.text()}`);
}

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
// A second long room that no step opens before the scroll test, so its history is still on the server and
// scrolling up has to page it in.
const deepRoom = await api('/createRoom', { name: 'Deep History', invite: [u.id] }, v.token);
await api(`/join/${encodeURIComponent(deepRoom.room_id)}`, {}, u.token);
for (let i = 1; i <= 160; i++) {
  await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(deepRoom.room_id)}/send/m.room.message/d${i}`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
    body: JSON.stringify({ msgtype: 'm.text', body: `deep ${String(i).padStart(3, '0')}` }),
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
  // A page error is a bug even when every step passed: collect them and fail at the end.
  const pageErrors = [];
  page.on('pageerror', (e) => {
    pageErrors.push(e.message);
    console.error('pageerror:', e.message);
  });
  const assertReactionOverlay = async (rowSelectorOrHandle) => {
    await page.waitForFunction((target) => {
      const isBubbles = document.documentElement.getAttribute('data-message-layout') === 'bubbles';
      let row = typeof target === 'string' ? document.querySelector(target) : target;
      if (!row && typeof target === 'string') {
        row = [...document.querySelectorAll('.row[data-event-id], .row')].find((r) => r.querySelector('.reactions .chip'));
      }
      if (!row) return false;
      const rowBubbles = isBubbles || row.getAttribute('data-layout') === 'bubbles';
      if (!rowBubbles) return true;
      const bubble = row.querySelector('.bubble');
      const pill = row.querySelector('.chip .pill') || row.querySelector('.chip');
      if (!bubble || !pill) return false;
      const bRect = bubble.getBoundingClientRect();
      const pRect = pill.getBoundingClientRect();
      const pillCenter = pRect.top + pRect.height / 2;
      return Math.abs(pillCenter - bRect.bottom) <= 4;
    }, { timeout: 10000 }, rowSelectorOrHandle);
  };
  page.on('console', (m) => ['error', 'warning', 'log'].includes(m.type()) && console.error('console:', m.text()));
  if (process.env.E2E_TRACE) {
    const cdp = await page.createCDPSession();
    await cdp.send('Network.enable');
    cdp.on('Network.webSocketFrameSent', (e) => console.log('>>', e.response.payloadData.slice(0, 300)));
    cdp.on('Network.webSocketFrameReceived', (e) => { const d = e.response.payloadData; if (!d.includes('RoomListDiff')) console.log('<<', d.slice(0, 600)); });
  }
  await page.goto(`http://localhost:${UI_PORT}/?engine=ws://127.0.0.1:${BRIDGE_PORT}&token=${BRIDGE_TOKEN}`, { waitUntil: 'networkidle0' });

  step('onboarding: discover server, sign in');
  // A fresh profile opens on the welcome step; acknowledging it is what reveals the server field.
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Get started'), { timeout: 20000 });
  await page.evaluate(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Get started').click());
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

  // First-run permissions gate: if it appears on a fresh profile, verify and dismiss it before interaction
  const hasGate = await page.waitForFunction(() => {
    return document.querySelector('[role=dialog][aria-label="Allow what Alloy needs"], .gate') !== null;
  }, { timeout: 3000 }).catch(() => false);

  if (hasGate) {
    step('first-run permissions gate: assert summary and rows, then dismiss');
    await page.waitForFunction(() => {
      const summary = document.querySelector('[data-testid="permissions-summary"]')?.textContent?.trim();
      const rows = document.querySelectorAll('.gate li[data-permission], [role=dialog] li[data-permission]');
      return rows.length > 0 && summary && !summary.includes('Checking');
    }, { timeout: 5000 });

    const gateData = await page.evaluate(() => {
      const gateEl = document.querySelector('[role=dialog][aria-label="Allow what Alloy needs"], .gate');
      const summary = gateEl?.querySelector('[data-testid="permissions-summary"]')?.textContent?.trim() || '';
      const rows = [...(gateEl?.querySelectorAll('li[data-permission]') ?? [])].map((r) => ({
        id: r.getAttribute('data-permission'),
        state: r.getAttribute('data-state'),
      }));
      return { summary, rows };
    });

    if (!gateData.summary) throw new Error('permissions gate summary rendered empty');
    if (!gateData.rows.length || !gateData.rows.some((r) => r.id === 'notifications')) {
      throw new Error(`permissions gate rows missing notifications: ${JSON.stringify(gateData.rows)}`);
    }

    await page.evaluate(() => {
      const d = document.querySelector('[role=dialog][aria-label="Allow what Alloy needs"], .gate');
      const btn = [...(d?.querySelectorAll('button') ?? [])].find((b) => b.textContent.trim() === 'Continue');
      if (!btn) throw new Error('Continue button not found in permissions gate');
      btn.click();
    });
    await page.waitForFunction(
      () => !document.querySelector('[role=dialog][aria-label="Allow what Alloy needs"], .gate'),
      { timeout: 10000 }
    );
  }

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
    step('mobile: long-press a message floats the quick reactions beside it');
    await settle();
    const cdp = await page.createCDPSession();
    const pressAt = await page.evaluate(() => { const r = [...document.querySelectorAll('.row[data-event-id]')].find((e) => e.textContent.includes('Hi from bob')).querySelector('.body').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [pressAt] });
    await new Promise((r) => setTimeout(r, 800));
    await page.waitForSelector('.reaction-float button[aria-label^="React with"]', { visible: true, timeout: 5000 });
    const floatBox = await page.$eval('.reaction-float', (el) => {
      const b = el.getBoundingClientRect();
      const bottomLimit = document.querySelector('.phone-action-bar, .context-menu.sheet')?.getBoundingClientRect().top ?? 780;
      return { l: b.left, r: b.right, t: b.top, b: b.bottom, bottomLimit };
    });
    if (floatBox.l < 0 || floatBox.r > 390 || floatBox.t < 0 || floatBox.b > floatBox.bottomLimit) {
      throw new Error(`reaction pill outside its area: ${JSON.stringify(floatBox)}`);
    }
    await audit('2b-long-press');
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await page.waitForSelector('.reaction-float button[aria-label^="React with"]', { visible: true, timeout: 5000 });
    const laugh = await page.evaluate(() => { const b = [...document.querySelectorAll('.reaction-float button[aria-label^="React with"]')].find((e) => e.textContent.includes('\u{1F602}')).getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; });
    await page.touchscreen.tap(laugh.x, laugh.y);
    await page.waitForFunction(() => !document.querySelector('.sheet-scrim'), { timeout: 5000 });
    await page.waitForFunction(() => [...document.querySelectorAll('.row[data-event-id]')].find((e) => e.textContent.includes('Hi from bob'))?.querySelector('button[aria-label^="\u{1F602}"]'), { timeout: 20000 });
    await assertReactionOverlay('.row[data-event-id]:has(button[aria-label^="\u{1F602}"])');
    await settle();
    step('mobile: swipe a message to the right replies to it');
    const swipeAt = await page.evaluate(() => { const r = [...document.querySelectorAll('.row[data-event-id]')].find((e) => e.textContent.includes('Hi from bob')).getBoundingClientRect(); return { x: 60, y: r.y + r.height / 2 }; });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [swipeAt] });
    for (let dx = 10; dx <= 110; dx += 10) { await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: swipeAt.x + dx, y: swipeAt.y }] }); await new Promise((r) => setTimeout(r, 16)); }
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await page.waitForFunction(() => document.querySelector('.composer .banner')?.textContent.includes('Replying to'), { timeout: 5000 });
    await page.click('.composer .banner button[aria-label="Cancel reply"]'); await settle();
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
    await tapSel('button[aria-label="New room or browse"]'); await audit('6-new-room'); await page.keyboard.press('Escape'); await settle();
    await page.evaluate(() => [...document.querySelectorAll('nav[aria-label=Primary] button')].find((b) => b.textContent.includes('Discover')).click()); await settle(); await audit('7-directory'); await page.keyboard.press('Escape'); await settle();
    await page.evaluate(() => [...document.querySelectorAll('nav[aria-label=Primary] button')].find((b) => b.textContent.includes('Settings')).click()); await settle(); await audit('8-settings-nav');
    await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.trim() === 'Appearance').click()); await settle();
    await audit('9-settings-appearance');
    await page.evaluate(() => document.querySelector('button[aria-label="Back to settings"]').click()); await settle();
    await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => b.textContent.trim() === 'Help & About').click()); await settle();
    await audit('10-settings-about');
    for (const message of new Set(pageErrors)) problems.push(`page error: ${message}`);
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
  await page.evaluate((row) => row.querySelector('button[aria-label="More reactions"]').click(), bobRow);
  // The shared emoji picker: search narrows the grid, picking an emoji toggles the reaction.
  await page.waitForSelector('.emoji-picker .cell', { visible: true, timeout: 30000 });
  await uxShot(page, 'emoji-picker');
  await page.type('.emoji-picker input[aria-label="Search emoji"]', 'thumbs up');
  await page.waitForFunction(() => document.querySelector('.emoji-picker .heading')?.textContent === 'Results' && /thumbs up/i.test(document.querySelector('.emoji-picker .cell')?.getAttribute('aria-label') ?? ''), { timeout: 10000 });
  await page.evaluate(() => document.querySelector('.emoji-picker .cell').click());
  await page.waitForFunction(() => document.querySelector('.chip.mine'), { timeout: 30000 });
  await assertReactionOverlay(bobRow);

  step('one-click quick reaction from the hover toolbar');
  await page.waitForFunction(() => [...document.querySelectorAll('.row[data-event-id]')].some((r) => r.textContent.includes('alloy-e2e') && r.querySelector('.toolbar')), { timeout: 30000 });
  const linkRow = await page.evaluateHandle(() => [...document.querySelectorAll('.row[data-event-id]')].find((r) => r.textContent.includes('alloy-e2e')));
  await linkRow.asElement().hover();
  await page.evaluate((row) => row.querySelector('.toolbar button[aria-label^="React with"]').click(), linkRow);
  await page.waitForFunction((row) => row.querySelector('.chip.mine'), { timeout: 30000 }, linkRow);
  await assertReactionOverlay(linkRow);

  step('live: a message sent by the other user while the room is open arrives without reopening');
  await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(room.room_id)}/send/m.room.message/live1`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
    body: JSON.stringify({ msgtype: 'm.text', body: 'live ping alloy-live' }),
  });
  await page.waitForFunction(() => document.body.innerText.includes('live ping alloy-live'), { timeout: 60000 });

  step('read receipts: the tick doubles once the other user has read my message');
  await page.waitForFunction(() => document.querySelector('.row.own[data-event-id]'), { timeout: 30000 });
  const ownEventId = await page.evaluate(() => [...document.querySelectorAll('.row.own[data-event-id]')].pop().dataset.eventId);
  await page.waitForFunction(() => document.querySelector('.row.own[data-event-id] .tick'), { timeout: 30000 });
  await api(`/rooms/${encodeURIComponent(room.room_id)}/receipt/m.read/${encodeURIComponent(ownEventId)}`, {}, v.token);
  await page.waitForFunction((id) => document.querySelector(`.row[data-event-id="${CSS.escape(id)}"] .tick.read`), { timeout: 30000 }, ownEventId);

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
  // Identity is read-first: the note becomes an editor only after its pencil is pressed.
  await page.click('section[aria-label="Room info"] button[aria-label="Edit private note"]');
  await page.waitForSelector('section[aria-label="Room info"] textarea[aria-label="Private note"]', { visible: true, timeout: 15000 });
  await page.evaluate(() => {
    const note = document.querySelector('section[aria-label="Room info"] textarea[aria-label="Private note"]');
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
  // Deleting asks first: Matrix removes the message for everyone.
  await page.waitForSelector('[role=dialog][aria-label="Delete for everyone?"], [role=dialog]', { visible: true, timeout: 10000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog] button')].find((b) => /^Delete$/.test(b.textContent.trim())).click());
  await page.waitForFunction(() => document.body.innerText.includes('Message deleted'), { timeout: 30000 });

  step('invitation: accept it from the room list');
  await page.waitForFunction(() => [...document.querySelectorAll('.row.invite')].some((e) => e.textContent.includes('Invite me')), { timeout: 30000 });
  await page.evaluate(() => [...document.querySelectorAll('.row.invite')].find((e) => e.textContent.includes('Invite me')).querySelector('button').click());
  await page.waitForFunction(() => [...document.querySelectorAll('button.row')].some((e) => e.textContent.includes('Invite me')), { timeout: 30000 });

  step('create an encrypted room from the UI and send a message in it');
  await page.click('button[aria-label="New room or browse"]');
  await page.waitForSelector('[role=menuitem]', { visible: true, timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((b) => b.textContent.includes('New room')).click());
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


  // A second room with U as admin (unencrypted), for pins, forwarding, favourites, drafts and message search.
  step('pins: pin a message, see the pinned bar, unpin from the list');
  // Opens the context menu on `selector` and clicks `label` in the same page task, so a menu that closes
  // on its own (layout shifts while images load) cannot slip away between finding the entry and clicking it.
  const menuOn = async (selector, label) => {
    let seen = [];
    for (let attempt = 0; attempt < 4; attempt++) {
      const result = await page.evaluate(async (sel, text) => {
        const el = document.querySelector(sel);
        if (!el) return { clicked: false, seen: ['(no element matches the selector)'] };
        const r = el.getBoundingClientRect();
        el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10 }));
        const entries = () => [...document.querySelectorAll('[role=menu] [role^=menuitem]')];
        for (let i = 0; i < 40; i++) {
          await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 25)));
          const hit = entries().find((e) => e.textContent.trim() === text);
          if (hit) {
            hit.click();
            return { clicked: true, seen: [] };
          }
        }
        return { clicked: false, seen: entries().map((e) => e.textContent.trim()) };
      }, selector, label);
      if (result.clicked) return;
      seen = result.seen;
      await page.keyboard.press('Escape');
    }
    throw new Error(`menu has no "${label}" entry; it offers: ${seen.join(' | ') || 'nothing'}`);
  };
  const rowKey = async (title) => page.evaluate((t) => { const el = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes(t)); el.setAttribute('data-e2e', t); return t; }, title);
  // The list is rebuilt as chats move between sections: wait for the row and click it in one page task, so a
  // list that is mid-update cannot make the row vanish between finding it and clicking it (nor shift under a pointer).
  const clickRow = (title) =>
    page.waitForFunction(
      (t) => {
        const el = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes(t));
        if (!el) return false;
        el.click();
        return true;
      },
      { timeout: 15000 },
      title,
    );
  // Opens a chat and waits for its header; if the click was swallowed (a dialog still closing, a list mid-update)
  // it clicks again, so a lost click shows up as a clear error instead of a later timeout.
  const openRoom = async (title) => {
    for (let attempt = 0; attempt < 4; attempt++) {
      await clickRow(title);
      try {
        await page.waitForSelector(`button[aria-label="Room info for ${title}"]`, { timeout: 4000 });
        return;
      } catch {
        // try the click again
      }
    }
    throw new Error(`the chat "${title}" did not open after four clicks`);
  };
  await openRoom('Pin Room');
  await page.waitForFunction(() => document.body.innerText.includes('pin target alloy-pin'), { timeout: 60000 });
  const markMsg = () => page.evaluate(() => { const r = [...document.querySelectorAll('.row[data-event-id]')].find((x) => x.textContent.includes('pin target alloy-pin')); r.setAttribute('data-e2e-msg', '1'); });
  await markMsg();
  await menuOn('.row[data-e2e-msg]', 'Pin');
  await page.waitForFunction(() => document.querySelector('[role=region][aria-label="Pinned messages"]')?.innerText.includes('Pinned message'), { timeout: 30000 });
  await uxShot(page, 'pinned-bar');

  step('albums: pictures sent together form one grid that opens a gallery');
  await page.waitForFunction(() => document.querySelectorAll('.album .tile').length === 3, { timeout: 30000 });
  await uxShot(page, 'album');
  // A click at coordinates can land on the pinned bar that appeared just above; a DOM click goes to the tile.
  await page.evaluate(() => document.querySelector('.album .tile').click());
  await page.waitForFunction(() => document.querySelector('[role=dialog][aria-modal=true]')?.innerText.includes('1 / 3'), { timeout: 30000 });
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => document.querySelector('[role=dialog][aria-modal=true]')?.innerText.includes('2 / 3'), { timeout: 10000 });
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('[role=dialog][aria-modal=true]'), { timeout: 10000 });

  step('mentions: a message that mentions me carries the mention edge');
  await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(pinRoom.room_id)}/send/m.room.message/m1`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
    body: JSON.stringify({ msgtype: 'm.text', body: `hey ${uName} alloy-mention`, 'm.mentions': { user_ids: [u.id] } }),
  });
  await page.waitForFunction(() => [...document.querySelectorAll('.row.mention')].some((r) => r.textContent.includes('alloy-mention')), { timeout: 60000 });
  await page.click('[role=region][aria-label="Pinned messages"] button[aria-label="All pinned messages"]');
  await page.waitForSelector('[role=dialog] button[aria-label="Unpin message"]', { visible: true, timeout: 10000 });
  await page.click('[role=dialog] button[aria-label="Unpin message"]');
  await page.waitForFunction(() => !document.querySelector('[role=region][aria-label="Pinned messages"]'), { timeout: 30000 });
  await page.keyboard.press('Escape');

  step('forward a message to another room');
  await menuOn('.row[data-e2e-msg]', 'Forward');
  await page.waitForSelector('[role=dialog] .room', { visible: true, timeout: 10000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=dialog] .room')].find((r) => r.textContent.includes('From Bob')).querySelector('button').click());
  await page.waitForFunction(() => [...document.querySelectorAll('[role=dialog] .room')].find((r) => r.textContent.includes('From Bob'))?.querySelector('button')?.textContent.trim() === 'Sent', { timeout: 30000 });
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('[role=dialog]'), { timeout: 10000 });
  await openRoom('From Bob');
  await page.waitForFunction(() => [...document.querySelectorAll('.row[data-event-id]')].some((r) => r.textContent.includes('pin target alloy-pin')), { timeout: 60000 });

  // Rows are rebuilt when they move between sections, so each helper marks the row again first.
  const roomMenuOn = async (title, label) => {
    await rowKey(title);
    await menuOn(`button.row[data-e2e="${title}"]`, label);
  };
  const clickRoom = async (title) => {
    await openRoom(title);
  };
  // Primary filters are tabs; every filter, Archive included, lives in the room-list options menu.
  const chip = (name) => menuOn('.filter-more', name);
  // A tag or unread flag is confirmed by the server and comes back through sync, which can take a while on a busy runner.
  const SYNC = 60000;
  const hasRow = (title) => page.waitForFunction((t) => [...document.querySelectorAll('button.row')].some((e) => e.textContent.includes(t)), { timeout: SYNC }, title);

  step('chat list: pin to top, archive, mark as unread');
  await roomMenuOn('Pin Room', 'Pin to top');
  await page.waitForFunction(() => document.querySelector('section[aria-label="Pinned"]')?.innerText.includes('Pin Room'), { timeout: SYNC });
  await roomMenuOn('Pin Room', 'Unpin from top');
  await page.waitForFunction(() => !document.querySelector('section[aria-label="Pinned"]'), { timeout: SYNC });
  await roomMenuOn('Pin Room', 'Archive');
  await page.waitForFunction(() => ![...document.querySelectorAll('button.row')].some((e) => e.textContent.includes('Pin Room')), { timeout: SYNC });
  await chip('Archive');
  await hasRow('Pin Room');
  await roomMenuOn('Pin Room', 'Unarchive');
  await chip('All');
  await hasRow('Pin Room');
  await roomMenuOn('Pin Room', 'Mark as unread');
  await page.waitForFunction(() => !![...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Pin Room'))?.querySelector('.dot.accent'), { timeout: SYNC });
  await clickRoom('Pin Room');
  await page.waitForFunction(() => ![...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Pin Room'))?.querySelector('.dot.accent'), { timeout: SYNC });

  step('drafts: unsent text shows in the list and returns when you come back');
  await page.click('textarea[aria-label^=Message]');
  await page.type('textarea[aria-label^=Message]', 'my unsent thought');
  await clickRoom('From Bob');
  await rowKey('Pin Room');
  await page.waitForFunction(() => document.querySelector('button.row[data-e2e="Pin Room"] .draft') && document.querySelector('button.row[data-e2e="Pin Room"]').innerText.includes('my unsent thought'), { timeout: 15000 });
  await clickRoom('Pin Room');
  await page.waitForFunction(() => document.querySelector('textarea[aria-label^=Message]').value === 'my unsent thought', { timeout: 15000 });
  await page.$eval('textarea[aria-label^=Message]', (el) => { el.value = ''; el.dispatchEvent(new Event('input', { bubbles: true })); });

  step('search all chats: a message from another room is found and opens its room');
  await clickRoom('From Bob');
  await page.waitForFunction(() => document.querySelector('textarea[aria-label^=Message]'), { timeout: 15000 });
  await page.type('input[aria-label^="Search rooms"]', 'alloy-pin');
  await page.waitForFunction(() => [...document.querySelectorAll('section[aria-label="Messages"] .hit')].some((b) => b.textContent.includes('Pin Room')), { timeout: 30000 });
  await page.evaluate(() => [...document.querySelectorAll('section[aria-label="Messages"] .hit')].find((b) => b.textContent.includes('Pin Room')).click());
  await page.waitForFunction(() => document.querySelector('button[aria-label="Room info for Pin Room"]'), { timeout: 30000 });
  await page.$eval('input[aria-label^="Search rooms"]', (el) => { el.value = ''; el.dispatchEvent(new Event('input', { bubbles: true })); });
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

  step('scrolling up through history keeps the message you are reading in place and adds no new-message count');
  await page.waitForFunction(() => [...document.querySelectorAll('button.row .title')].some((e) => e.textContent === 'Deep History'), { timeout: 60000 });
  await page.evaluate(() => [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Deep History')).click());
  await page.waitForSelector('button[aria-label="Room info for Deep History"]', { timeout: 15000 });
  await page.waitForFunction(() => document.querySelectorAll('section.timeline .row[data-event-id]').length > 3, { timeout: 30000 });
  // Chrome keeps the view steady on its own (scroll anchoring); macOS and Linux WebKit do not. Switch it off so the
  // test exercises the app's own handling of prepended history.
  await page.addStyleTag({ content: '* { overflow-anchor: none !important; }' });
  // A user starts scrolling once the chat has appeared and stopped moving; settle on that, then scroll with a real wheel.
  let lastTop = -1;
  for (let stable = 0, n = 0; stable < 4 && n < 80; n++) {
    await new Promise((r) => setTimeout(r, 150));
    const top = await page.evaluate(() => document.querySelector('section.timeline .viewport > div')?.scrollTop ?? -1);
    stable = top === lastTop && top > 0 ? stable + 1 : 0;
    lastTop = top;
  }
  if (await page.evaluate(() => document.body.innerText.includes('deep 001'))) throw new Error('the oldest message was already loaded, so scrolling up would page in nothing');
  const samples = [];
  for (let i = 0; i < 40; i++) {
    // The row nearest the middle of the view stands for "the message being read".
    const anchor = await page.evaluate(() => {
      const view = document.querySelector('section.timeline .viewport').getBoundingClientRect();
      const mid = view.top + view.height / 2;
      const rows = [...document.querySelectorAll('section.timeline .row[data-event-id]')];
      const row = rows.sort((a, b) => Math.abs(a.getBoundingClientRect().top - mid) - Math.abs(b.getBoundingClientRect().top - mid))[0];
      const v = document.querySelector('section.timeline .viewport > div');
      return { id: row.dataset.eventId, top: row.getBoundingClientRect().top - view.top, scrollTop: v.scrollTop, size: v.scrollHeight };
    });
    const asked = Math.min(500, anchor.scrollTop);
    // A real wheel event, like a user's: programmatic scrollTop writes do not cancel a scroll-to-bottom still running.
    const box = await (await page.$('section.timeline .viewport')).boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel({ deltaY: -500 });
    await new Promise((r) => setTimeout(r, 1000));
    const moved = await page.evaluate(({ id }) => {
      const view = document.querySelector('section.timeline .viewport').getBoundingClientRect();
      const row = document.querySelector(`section.timeline .row[data-event-id="${CSS.escape(id)}"]`);
      return row ? { top: row.getBoundingClientRect().top - view.top, size: document.querySelector('section.timeline .viewport > div').scrollHeight } : null;
    }, anchor);
    // The row must slide down by what was scrolled, and not by the height of history loaded above it.
    if (moved !== null) samples.push({ i, asked: Math.round(asked), error: Math.round(moved.top - anchor.top - asked), scrollTop: Math.round(anchor.scrollTop), badge: await page.evaluate(() => document.querySelector('.fabs .badge')?.textContent ?? null) });
    if (await page.evaluate(() => document.body.innerText.includes('deep 001')) && anchor.scrollTop === 0) break;
  }
  console.log('scroll anchoring samples:', JSON.stringify(samples));
  const worst = samples.reduce((m, s) => Math.max(m, Math.abs(s.error)), 0);
  if (!(await page.evaluate(() => document.body.innerText.includes('deep 001')))) throw new Error(`scrolling up never paged in the oldest message: ${JSON.stringify(samples)}`);
  if (samples.length < 3) throw new Error(`the scroll test measured only ${samples.length} scrolls`);
  if (worst > 60) throw new Error(`the message being read jumped by ${worst}px while older history loaded: ${JSON.stringify(samples)}`);
  const counted = samples.find((s) => s.badge);
  if (counted) throw new Error(`scrolling up with no new message showed a new-message count of ${counted.badge}`);
  // The count must still work for what it is for: a message from someone else while scrolled up away from the bottom.
  for (const n of [1, 2]) {
    await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(deepRoom.room_id)}/send/m.room.message/newer${n}`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
      body: JSON.stringify({ msgtype: 'm.text', body: `deep newer ${n}` }),
    });
  }
  await page.waitForFunction(() => document.querySelector('.fabs .badge')?.textContent === '2', { timeout: 30000 });
  // Switching rooms from a scrolled-up chat must land on the newest message of the other room, with no leftover count.
  await page.evaluate(() => [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('Pin Room')).click());
  await page.waitForSelector('button[aria-label="Room info for Pin Room"]', { timeout: 15000 });
  await new Promise((r) => setTimeout(r, 1500));
  const switched = await page.evaluate(() => {
    const v = document.querySelector('section.timeline .viewport > div');
    return { gap: v ? v.scrollHeight - v.scrollTop - v.clientHeight : null, badge: document.querySelector('.fabs .badge')?.textContent ?? null };
  });
  if (switched.gap === null || switched.gap > 80 || switched.badge) throw new Error(`the next room did not open at its newest message: ${JSON.stringify(switched)}`);

  step('browse the public room directory');
  await page.click('button[aria-label="New room or browse"]');
  await page.waitForSelector('[role=menuitem]', { visible: true, timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('[role=menuitem]')].find((b) => b.textContent.includes('Browse public rooms')).click());
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

  step('right-click a message: the reaction strip above the menu applies a reaction');
  await openRoom('From Bob');
  await page.waitForFunction(() => [...document.querySelectorAll('.row[data-event-id]')].some((e) => e.textContent.includes('live ping alloy-live')), { timeout: 30000 });
  const liveBox = await page.evaluate(() => { const r = [...document.querySelectorAll('.row[data-event-id]')].find((e) => e.textContent.includes('live ping alloy-live')).querySelector('.body').getBoundingClientRect(); return { x: r.x + 20, y: r.y + r.height / 2 }; });
  await page.mouse.click(liveBox.x, liveBox.y, { button: 'right' });
  await page.waitForSelector('.desktop-reaction-strip button[aria-label^="React with"]', { visible: true, timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('.desktop-reaction-strip button[aria-label^="React with"]')].find((e) => e.textContent.includes('\u{1F62E}')).click());
  await page.waitForFunction(() => [...document.querySelectorAll('.row[data-event-id]')].find((e) => e.textContent.includes('live ping alloy-live'))?.querySelector('button[aria-label^="\u{1F62E}"]'), { timeout: 20000 });
  await assertReactionOverlay('.row[data-event-id]:has(button[aria-label^="\u{1F62E}"])');

  step('refresh: restart sync from the header button; the open chat stays open and later messages still arrive');
  await openRoom('From Bob');
  await page.click('button[aria-label="Refresh"]');
  await page.waitForFunction(() => !document.querySelector('.refreshing') && !document.querySelector('button[aria-label="Refresh"]')?.disabled, { timeout: 30000 });
  await page.waitForSelector('button[aria-label="Room info for From Bob"]', { timeout: 15000 });
  await page.waitForFunction(() => document.body.innerText.includes('live ping alloy-live'), { timeout: 30000 });
  await fetch(`${HS}/_matrix/client/v3/rooms/${encodeURIComponent(room.room_id)}/send/m.room.message/live2`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${v.token}` },
    body: JSON.stringify({ msgtype: 'm.text', body: 'after refresh alloy-refresh' }),
  });
  await page.waitForFunction(() => document.body.innerText.includes('after refresh alloy-refresh'), { timeout: 60000 });

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
  // For a reacted message in bubbles layout, the pill's vertical center should be within 4px of the bubble's bottom edge
  await assertReactionOverlay('.row:has(.reactions .chip)');
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
  step('settings: open permissions and assert rows and summary');
  await openPage('Permissions');
  await page.waitForFunction(() => {
    const dialog = document.querySelector('[role=dialog]');
    const summary = dialog?.querySelector('[data-testid="permissions-summary"]')?.textContent?.trim();
    const rows = dialog?.querySelectorAll('li[data-permission]');
    return !!summary && !summary.includes('Checking') && (rows?.length ?? 0) > 0;
  }, { timeout: 15000 });
  await uxShot(page, 'settings-permissions');
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
  if (pageErrors.length) throw new Error(`the page reported ${pageErrors.length} error(s): ${[...new Set(pageErrors)].join(' | ')}`);
  console.log('E2E OK');
} catch (e) {
  failed = e;
  const frames = (e.stack || '').split('\n');
  const where = frames.filter((l) => l.includes('e2e-bridge.mjs')).slice(0, 2).map((l) => l.trim().replace(/^.*\//, '')).join(' | ');
  console.error('E2E FAILED:', e.message, `[at ${where || 'unknown step'}]`, frames.slice(1, 4).join(' | '));
  try {
    const pages = await browser.pages();
    const text = await pages[pages.length - 1].evaluate(() => `${document.querySelectorAll('.album .tile').length} album tiles; rows=${document.querySelectorAll('.row[data-event-id]').length} own=${document.querySelectorAll('.row.own[data-event-id]').length}; timeline: ${(document.querySelector('section.timeline')?.innerText ?? '').replace(/\s+/g, ' ').slice(0, 500)}; dialogs: ${JSON.stringify([...document.querySelectorAll('[role=dialog]')].map((d) => d.innerText.replace(/\s+/g, ' ').slice(0, 200)))}; page: ${document.body.innerText.replace(/\s+/g, ' ').slice(0, 300)}`);
    console.error('E2E page:', text);
  } catch {}
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
