#!/usr/bin/env node
/**
 * Regenerates the README screenshots from the UI running on its demo engine (no server, no account):
 * `node scripts/readme-shots.mjs`. Needs Chrome (CHROME_PATH to override).
 * Output: docs/assets/screenshots/*.png
 */
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startDevServer, waitForShell } from './dev-server.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const out = resolve(__dirname, '../../../../docs/assets/screenshots');
const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5198;
fs.mkdirSync(out, { recursive: true });

// The demo engine is a development-only path (see src/lib/ipc/index.ts), so run the dev server.
const server = await startDevServer(PORT);
const url = server.url;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const browser = await puppeteer.launch({ executablePath: CHROME, headless: 'new', args: ['--no-sandbox'] });
const shot = async (page, name) => {
  await page.mouse.move(700, 600); // no stray hover tooltips in the picture
  await sleep(200);
  await page.screenshot({ path: resolve(out, `${name}.png`) });
};
const clickText = (page, selector, text) =>
  page.evaluate((s, t) => [...document.querySelectorAll(s)].find((e) => e.textContent.includes(t))?.click(), selector, text);

try {
  // --- desktop
  const page = await browser.newPage();
  await page.setViewport({ width: 1360, height: 840, deviceScaleFactor: 2 });
  await page.goto(url);
  await waitForShell(page);
  await sleep(800);
  await clickText(page, 'button.row', 'Alloy Core Dev');
  await sleep(1200);
  await shot(page, 'chat');

  await page.click('button[aria-label="Search in room"]');
  await page.waitForSelector('aside[aria-label="Search in room"] input', { visible: true });
  await page.type('aside[aria-label="Search in room"] input', 'release');
  await sleep(1200);
  await shot(page, 'search');
  await page.click('button[aria-label="Close search"]');
  await sleep(300);

  await page.click('button[aria-label="Insert emoji"]');
  await page.waitForSelector('.emoji-picker .cell', { visible: true, timeout: 20000 });
  await sleep(600);
  await shot(page, 'emoji');
  await page.keyboard.press('Escape');
  await sleep(300);

  await page.evaluate(() => {
    const row = [...document.querySelectorAll('button.row')].find((e) => e.textContent.includes('General'));
    const r = row.getBoundingClientRect();
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 40, clientY: r.top + 20 }));
  });
  await page.waitForSelector('[role=menu] [role=menuitem]', { visible: true });
  await clickText(page, '[role=menuitem]', 'Notifications');
  await sleep(500);
  await shot(page, 'room-menu');
  await page.keyboard.press('Escape');
  await sleep(300);

  await page.click('button[aria-label="Settings"]');
  await page.waitForSelector('[role=dialog]');
  await clickText(page, '[role=dialog] nav button', 'Appearance');
  await sleep(700);
  await shot(page, 'settings');
  await page.close();

  // --- phone
  const phone = await browser.newPage();
  await phone.setViewport({ width: 390, height: 844, deviceScaleFactor: 2, isMobile: true, hasTouch: true });
  await phone.goto(url);
  await waitForShell(phone);
  await sleep(800);
  await shot(phone, 'phone-chats');
  await clickText(phone, 'button.row', 'Alloy Core Dev');
  await sleep(1200);
  await shot(phone, 'phone-chat');
  await phone.close();
  console.log(`screenshots written to ${out}`);
} finally {
  await browser.close();
  server.stop();
}
