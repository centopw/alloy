#!/usr/bin/env node
/**
 * ADR 0014 review artifact: the conversation shell at 1280 / 900 / 390 in the three shipped
 * themes, so the material layers, secondary text and hairlines can be checked where the aurora
 * is brightest.
 *
 *   node scripts/screenshot-adr-0014.mjs
 *
 * Needs Chrome (`CHROME_PATH` to override). Writes docs/spikes/adr-0014/<theme>-<width>.png.
 * Runs against the dev server: the demo engine is a development-only path (see ipc/index.ts).
 */
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startDevServer, waitForShell } from './dev-server.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '../../../..');
const out = resolve(repoRoot, 'docs/spikes/adr-0014');
const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5181;

const THEMES = ['alloy-light', 'alloy-dark', 'alloy-amoled'];
const VIEWPORTS = [
  { width: 1280, height: 900, phone: false },
  { width: 900, height: 900, phone: false },
  { width: 390, height: 844, phone: true },
];

fs.mkdirSync(out, { recursive: true });

const server = await startDevServer(PORT);
const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: 'new',
  args: ['--no-sandbox'],
});

try {
  for (const theme of THEMES) {
    for (const viewport of VIEWPORTS) {
      const page = await browser.newPage();
      await page.setViewport({
        width: viewport.width,
        height: viewport.height,
        deviceScaleFactor: viewport.phone ? 2 : 1,
        isMobile: viewport.phone,
        hasTouch: viewport.phone,
      });
      await page.goto(server.url, { waitUntil: 'networkidle0' });
      // The development engine starts signed in, so the shell renders without a homeserver.
      await waitForShell(page);
      await page.evaluate((id) => {
        document.documentElement.dataset.theme = id;
      }, theme);
      await page.click('.row-wrap .row');
      await page.waitForSelector('button[aria-label^="Room info for"]', { timeout: 15_000 });
      const file = resolve(out, `${theme}-${viewport.width}.png`);
      await page.screenshot({ path: file });
      console.log(`saved ${file}`);
      await page.close();
    }
  }
} finally {
  await browser.close();
  server.stop();
}
