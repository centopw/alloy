#!/usr/bin/env node
/** Drive the built UI against the mock engine and capture screens: chat, settings. */
import puppeteer from 'puppeteer-core';
import http from 'node:http';
import fs from 'node:fs';
import { resolve, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const build = resolve(__dirname, '../build');
const out = resolve(__dirname, '../../../../docs/spikes/ui');
const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png' };
fs.mkdirSync(out, { recursive: true });

const server = http.createServer((req, res) => {
  let p = resolve(build, '.' + decodeURIComponent(req.url.split('?')[0]));
  if (!p.startsWith(build) || !fs.existsSync(p) || fs.statSync(p).isDirectory()) p = resolve(build, 'index.html');
  res.writeHead(200, { 'content-type': types[extname(p)] ?? 'application/octet-stream' });
  fs.createReadStream(p).pipe(res);
});
await new Promise((r) => server.listen(5175, r));

const browser = await puppeteer.launch({ executablePath: CHROME, headless: 'new', args: ['--no-sandbox'] });
try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1280, height: 800 });
  page.on('pageerror', (e) => console.error('pageerror', e.message));
  page.on('console', (m) => m.type() === 'error' && console.error('console', m.text()));
  await page.goto('http://localhost:5175/', { waitUntil: 'networkidle0' });
  // The mock engine starts with a signed-in account, so the shell loads directly.
  await page.waitForSelector('.room', { timeout: 10000 });
  await page.click('.room');
  await new Promise((r) => setTimeout(r, 1200));
  await page.screenshot({ path: resolve(out, 'chat.png') });

  await page.click('.account');
  await page.waitForSelector('[role=dialog]');
  await page.screenshot({ path: resolve(out, 'settings.png') });
  console.log('screens saved in', out);
} finally {
  await browser.close();
  server.close();
}
