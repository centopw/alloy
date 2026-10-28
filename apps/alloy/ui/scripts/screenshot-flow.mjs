#!/usr/bin/env node
/** Drive the UI against the development engine and capture screens: chat, settings. */
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startDevServer, waitForShell } from './dev-server.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const out = resolve(__dirname, '../../../../docs/spikes/ui');
const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
fs.mkdirSync(out, { recursive: true });

// The demo engine is development-only (see src/lib/ipc/index.ts), so the script runs the dev server.
const server = await startDevServer(5175);

const browser = await puppeteer.launch({ executablePath: CHROME, headless: 'new', args: ['--no-sandbox'] });
try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1280, height: 800 });
  page.on('pageerror', (e) => console.error('pageerror', e.message));
  page.on('console', (m) => m.type() === 'error' && console.error('console', m.text()));
  await page.goto(server.url, { waitUntil: 'networkidle0' });
  await waitForShell(page);
  await page.click('.row-wrap .row');
  await new Promise((r) => setTimeout(r, 1200));
  await page.screenshot({ path: resolve(out, 'chat.png') });

  await page.click('button[aria-label="Settings"]');
  await page.waitForSelector('[role=dialog]');
  await page.screenshot({ path: resolve(out, 'settings.png') });
  console.log('screens saved in', out);
} finally {
  await browser.close();
  server.stop();
}
