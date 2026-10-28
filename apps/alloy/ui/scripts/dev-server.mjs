import { spawn } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const uiDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const viteBin = resolve(uiDir, 'node_modules/vite/bin/vite.js');

/**
 * Dismisses the first-run permissions gate when it opened over the shell.
 *
 * The gate reads permissions after mount, so it can appear a moment after the shell is interactive;
 * a click that lands on its backdrop is lost. Scripts dismiss it before they touch the UI.
 * Returns true when a gate was dismissed.
 */
export async function dismissPermissionGate(page) {
  const gate = await page
    .waitForSelector('[role=dialog][aria-label="Allow what Alloy needs"]', { timeout: 3000 })
    .catch(() => null);
  if (!gate) return false;
  await page.evaluate(() => {
    const dialog = document.querySelector('[role=dialog][aria-label="Allow what Alloy needs"]');
    [...(dialog?.querySelectorAll('button') ?? [])].find((b) => b.textContent.trim() === 'Continue')?.click();
  });
  await page.waitForFunction(
    () => !document.querySelector('[role=dialog][aria-label="Allow what Alloy needs"]'),
    { timeout: 10_000 }
  );
  return true;
}

/** Waits for the signed-in shell, then dismisses the permissions gate if it appeared. */
export async function waitForShell(page) {
  await page.waitForSelector('.row-wrap .row', { timeout: 60_000 });
  await dismissPermissionGate(page);
}

/**
 * Starts the SvelteKit dev server.
 *
 * The in-memory demo engine exists only in a development build (`import.meta.env.DEV`); a
 * production build shows the "no local engine" state instead. Scripts that need a populated UI
 * without a homeserver therefore run against the dev server, not `build/`.
 */
export async function startDevServer(port) {
  const child = spawn(process.execPath, [viteBin, 'dev', '--port', String(port), '--strictPort', '--host', '127.0.0.1'], {
    cwd: uiDir,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  child.stdout.on('data', (chunk) => (output += chunk));
  child.stderr.on('data', (chunk) => (output += chunk));

  const url = `http://127.0.0.1:${port}/`;
  const stop = () => child.kill('SIGTERM');
  const deadline = Date.now() + 60_000;
  for (;;) {
    if (child.exitCode !== null) {
      throw new Error(`vite dev exited with ${child.exitCode}:\n${output}`);
    }
    try {
      const response = await fetch(url);
      if (response.ok) return { url, stop };
    } catch {
      // Not listening yet.
    }
    if (Date.now() > deadline) {
      stop();
      throw new Error(`vite dev did not answer on ${url} within 60s:\n${output}`);
    }
    await new Promise((done) => setTimeout(done, 250));
  }
}
