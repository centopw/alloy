#!/usr/bin/env node
// Loads the production build (run `pnpm build` first) with Object.prototype frozen, as the Tauri
// webview does under `app.security.freezePrototype`, and fails when the page throws at startup.
// A dependency that assigns `exports.toString = ...` passes every other gate and ships a blank app.
import puppeteer from 'puppeteer-core';
import http from 'node:http';
import fs from 'node:fs';
import { resolve, extname } from 'node:path';
const root = resolve(import.meta.dirname, '../build');
const types = { '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml' };
const server = http.createServer((req, res) => {
  let p = resolve(root, '.' + req.url.split('?')[0]);
  if (!p.startsWith(root) || !fs.existsSync(p) || fs.statSync(p).isDirectory()) p = resolve(root, 'index.html');
  res.setHeader('content-type', types[extname(p)] ?? 'application/octet-stream');
  res.end(fs.readFileSync(p));
}).listen(0);
const url = `http://127.0.0.1:${server.address().port}/`;
const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true });
const page = await browser.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push('pageerror: ' + (e.stack ?? e.message ?? e)));
page.on('console', (m) => m.type() === 'error' && errors.push('console: ' + m.text()));
await page.evaluateOnNewDocument(() => Object.freeze(Object.prototype));
await page.goto(url, { waitUntil: 'networkidle0' });
// Outside Tauri the booted app lands on its "no engine" notice; a crashed one shows SvelteKit's 500.
const booted = await page.waitForSelector('main.splash.no-engine', { timeout: 5000 }).catch(() => null);
await browser.close();
server.close();
if (!booted || errors.length) {
  console.error('UI did not boot with a frozen Object.prototype:\n' + errors.join('\n'));
  process.exit(1);
}
console.log('ok: UI boots with a frozen Object.prototype');
