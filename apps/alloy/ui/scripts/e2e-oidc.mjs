#!/usr/bin/env node
/**
 * OIDC (MAS) sign-in round trip: real engine (alloy-bridge) + local MAS stack + Chrome.
 *
 * Covers what the Rust integration tests cannot: a human-shaped authorization step. The script
 * starts the login through the engine, signs in on the MAS page in Chrome, accepts the consent,
 * and asserts the engine's loopback listener exchanged the code and reported the account.
 *
 *   tests/mas/start.sh                       # the stack, homeserver at http://localhost:8089
 *   cargo build -p alloy-bridge
 *   ALLOY_BRIDGE_BIN=target/debug/alloy-bridge node scripts/e2e-oidc.mjs
 *
 * Environment: ALLOY_TEST_HS (default http://localhost:8089), CHROME_PATH, TEST_USER,
 * TEST_PASSWORD (defaults match tests/mas/start.sh).
 */
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import os from 'node:os';
import { spawn } from 'node:child_process';

const BRIDGE_BIN = process.env.ALLOY_BRIDGE_BIN ?? '/tmp/alloy-target-rel/debug/alloy-bridge';
const HS = process.env.ALLOY_TEST_HS ?? 'http://localhost:8089';
const CHROME =
  process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const USER = process.env.TEST_USER ?? 'alice';
const PW = process.env.TEST_PASSWORD ?? 'testpassword123!';
const PORT = 8799;
const TOKEN = 'oidc-check-token';
const dataDir = fs.mkdtempSync(`${os.tmpdir()}/alloy-oidc-`);
const log = (msg) => console.log(`   ${msg}`);

const wsEvents = [];
let ws;
let browser;
let bridge;

function invoke(command) {
  const id = Math.floor(Math.random() * 1e9);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout on ${command.action}`)), 60000);
    const onMessage = (m) => {
      const frame = JSON.parse(String(m.data));
      if (frame.id !== id) return;
      ws.removeEventListener('message', onMessage);
      clearTimeout(timer);
      if ('ok' in frame) resolve(frame.ok);
      else reject(Object.assign(new Error(frame.err.message), { code: frame.err.code, reason: frame.err.reason }));
    };
    ws.addEventListener('message', onMessage);
    ws.send(JSON.stringify({ id, command }));
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function clickIfPresent(page, pattern) {
  const clicked = await page.evaluate((source) => {
    const re = new RegExp(source, 'i');
    const button = [...document.querySelectorAll('button, a, input[type=submit]')].find(
      (el) => re.test(el.textContent || '') || re.test(el.value || '')
    );
    if (!button) return false;
    button.click();
    return true;
  }, pattern);
  return clicked;
}

try {
  bridge = spawn(BRIDGE_BIN, ['--port', String(PORT), '--data-dir', dataDir, '--token', TOKEN], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  bridge.stderr.on('data', (d) => process.env.DEBUG && console.error(`bridge: ${d}`));
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('bridge did not start')), 30000);
    bridge.stdout.on('data', (d) => {
      if (String(d).includes('listening')) {
        clearTimeout(timer);
        resolve();
      }
    });
    bridge.on('exit', (c) => reject(new Error(`bridge exited ${c}`)));
  });
  log(`bridge listening on ${PORT} (data ${dataDir})`);

  ws = new WebSocket(`ws://127.0.0.1:${PORT}/?token=${TOKEN}`);
  ws.addEventListener('message', (m) => {
    const frame = JSON.parse(String(m.data));
    if ('event' in frame) wsEvents.push(frame.event);
  });
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve);
    ws.addEventListener('error', () => reject(new Error('ws failed')));
  });

  const info = await invoke({ action: 'DiscoverHomeserver', params: { input: HS } });
  if (info.type !== 'HomeserverInfo') throw new Error(`unexpected discovery response ${info.type}`);
  log(`discovery: url=${info.payload.url} supports_oidc=${info.payload.supports_oidc}`);
  if (!info.payload.supports_oidc) throw new Error('server did not advertise OIDC');

  const start = await invoke({ action: 'StartOidcLogin', params: { homeserver_url: info.payload.url } });
  if (start.type !== 'OidcUrl') throw new Error(`unexpected start response ${start.type}`);
  log(`start: redirect_uri=${start.payload.redirect_uri}`);
  log(`authorize: ${start.payload.authorization_url.slice(0, 90)}…`);

  browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: 'new',
    args: ['--no-sandbox'],
  });
  const page = await browser.newPage();
  await page.goto(start.payload.authorization_url, { waitUntil: 'domcontentloaded' });

  // Sign in on the MAS page when it asks, then accept the consent page when one is shown.
  for (let step = 0; step < 8; step += 1) {
    if (new URL(page.url()).hostname === '127.0.0.1') break;
    const userField = await page.$('input[name="username"], input[autocomplete="username"]');
    if (userField) {
      const shown = await page.$eval('input[name="username"], input[autocomplete="username"]', (el) => el.value).catch(() => '');
      if (!shown) await userField.type(USER);
      const pwField = await page.$('input[type="password"]');
      if (pwField) await pwField.type(PW);
      log(`login page: submitting ${USER}`);
      await Promise.all([
        page.waitForNavigation({ waitUntil: 'domcontentloaded', timeout: 15000 }).catch(() => {}),
        (await page.$('button[type="submit"], input[type="submit"]'))?.click(),
      ]);
      await sleep(300);
      continue;
    }
    if (await clickIfPresent(page, 'allow|continue|authorize|approve|yes')) {
      log('consent page: accepted');
      await page.waitForNavigation({ waitUntil: 'domcontentloaded', timeout: 15000 }).catch(() => {});
      await sleep(300);
      continue;
    }
    await sleep(400);
  }

  await page.waitForFunction(() => location.hostname === '127.0.0.1', { timeout: 30000 });
  const body = await page.evaluate(() => document.body.innerText.trim());
  log(`callback page: ${body}`);

  const deadline = Date.now() + 20000;
  let account = null;
  while (Date.now() < deadline) {
    const state = wsEvents.find((e) => e.type === 'AuthState' && e.payload.account);
    if (state) {
      account = state.payload.account;
      break;
    }
    await sleep(200);
  }
  if (!account) throw new Error('no AuthState with an account arrived');
  log(`AuthState: ${account.user_id} @ ${account.homeserver_url}`);
  if (account.user_id !== '@alice:localhost') throw new Error(`unexpected user ${account.user_id}`);
  if (!body.includes('@alice:localhost')) throw new Error('callback page did not confirm the user');
  console.log('PASS OIDC authorization-code round trip completed in the engine');
} catch (err) {
  console.error(`FAIL ${err.message}`);
  process.exitCode = 1;
} finally {
  ws?.close();
  await browser?.close().catch(() => {});
  bridge?.kill();
  fs.rmSync(dataDir, { recursive: true, force: true });
}
