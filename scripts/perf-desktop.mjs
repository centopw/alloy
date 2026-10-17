#!/usr/bin/env node
/**
 * Cold start and idle memory benchmark for the release desktop app (macOS only).
 * Budgets and method: docs/plan/08-delivery-p1.md sections 2 and 3.
 *
 * Usage: node scripts/perf-desktop.mjs <app-binary> [--runs N]   (N defaults to 5)
 *
 * Per run:
 *   1. Spawn the binary and start a monotonic clock just before spawn.
 *   2. Poll System Events (about every 50 ms, bounded by osascript latency) until the
 *      app's process owns at least one window. Metric: "spawn to first window".
 *      If System Events window queries are not permitted, the whole benchmark uses
 *      the fallback metric "spawn to stable RSS": poll process-tree RSS until it is
 *      flat (max-min <= 2% of mean) over >= 500 ms; the metric is the time at which
 *      that flat period began.
 *   3. Sample idle process-tree RSS 10 s after ready: five samples at 1 s intervals.
 *      The tree is the app PID, its descendants, and WebKit XPC helpers that were not
 *      running before this launch (launchd parents those, so they are not children).
 *      The run's idle value is the median of its samples.
 *   4. Terminate the app, then wait for its helpers to exit before the next run.
 *
 * Progress goes to stderr. A JSON report goes to stdout. Exit status is 1 if any run failed.
 */
import { execFile, spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const execFileP = promisify(execFile);

const DEFAULT_RUNS = 5;
const COLD_START_BUDGET_MS = 2000;
const IDLE_RSS_BUDGET_MB = 150;
const IDLE_DELAY_MS = 10_000;
const IDLE_SAMPLES = 5;
const IDLE_STEP_MS = 1000;
const POLL_MS = 50;
const READY_TIMEOUT_MS = 60_000;
const FLAT_WINDOW_MS = 500;
const FLAT_TOLERANCE = 0.02;
const WEBKIT_RE = /WebKit\.framework|com\.apple\.WebKit\./;
const PERMISSION_RE = /not authori[sz]ed|not allowed|-1743|-25211|assistive access/i;

let activeChild = null;
process.on("exit", () => {
  if (activeChild !== null && isAlive(activeChild)) activeChild.kill("SIGKILL");
});
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => process.exit(130));
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const now = () => performance.now();
const log = (message) => console.error(`[perf-desktop] ${message}`);
const round1 = (x) => (x === null ? null : Math.round(x * 10) / 10);

function parseArgs(argv) {
  let binary = null;
  let runs = DEFAULT_RUNS;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--runs") runs = Number(argv[++i]);
    else if (arg.startsWith("--runs=")) runs = Number(arg.slice("--runs=".length));
    else if (arg.startsWith("-")) throw new Error(`unknown option: ${arg}`);
    else if (binary === null) binary = arg;
    else throw new Error(`unexpected argument: ${arg}`);
  }
  if (binary === null) throw new Error("usage: node scripts/perf-desktop.mjs <app-binary> [--runs N]");
  if (!Number.isInteger(runs) || runs < 1) throw new Error(`--runs must be a positive integer, got ${runs}`);
  return { binary, runs };
}

function median(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

// Nearest-rank percentile: with fewer than 20 runs this equals the maximum.
function p95(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.ceil(0.95 * sorted.length) - 1];
}

function summarize(values) {
  return { median: round1(median(values)), p95: round1(p95(values)) };
}

async function listProcesses() {
  const { stdout } = await execFileP("ps", ["-A", "-ww", "-o", "pid=,ppid=,rss=,command="]);
  const procs = [];
  for (const line of stdout.split("\n")) {
    const match = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/.exec(line);
    if (match) {
      procs.push({ pid: Number(match[1]), ppid: Number(match[2]), rssKb: Number(match[3]), command: match[4] });
    }
  }
  return procs;
}

/** PIDs of WebKit XPC helpers present in a process list. */
function webkitPids(procs) {
  return procs.filter((p) => WEBKIT_RE.test(p.command)).map((p) => p.pid);
}

/**
 * The app's process tree: the app PID, its descendants, and WebKit helpers that
 * were not running before this launch. Pre-existing WebKit processes (for example
 * Safari) are excluded from the baseline.
 */
function appTree(procs, rootPid, webkitBaseline) {
  const byPid = new Map(procs.map((p) => [p.pid, p]));
  const children = new Map();
  for (const p of procs) {
    const list = children.get(p.ppid);
    if (list) list.push(p.pid);
    else children.set(p.ppid, [p.pid]);
  }
  const seeds = [rootPid, ...webkitPids(procs).filter((pid) => !webkitBaseline.has(pid))];
  const seen = new Set();
  const stack = [...seeds];
  while (stack.length > 0) {
    const pid = stack.pop();
    if (seen.has(pid) || !byPid.has(pid)) continue;
    seen.add(pid);
    stack.push(...(children.get(pid) ?? []));
  }
  return [...seen].map((pid) => byPid.get(pid));
}

const sumMb = (procs) => procs.reduce((total, p) => total + p.rssKb, 0) / 1024;

function isAlive(child) {
  return child.pid !== undefined && child.exitCode === null && child.signalCode === null;
}

function assertAlive(child) {
  if (!isAlive(child)) {
    throw new Error(
      child.pid === undefined
        ? "failed to spawn the app"
        : `app exited before ready (code ${child.exitCode}, signal ${child.signalCode})`,
    );
  }
}

function waitExit(child, ms) {
  if (!isAlive(child)) return Promise.resolve(true);
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), ms);
    child.once("exit", () => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}

/** Resolves with the PIDs from `pids` still alive, polling until none remain or `ms` elapses. */
async function waitGone(pids, ms) {
  const deadline = now() + ms;
  for (;;) {
    const alive = new Set((await listProcesses()).map((p) => p.pid));
    const remaining = pids.filter((pid) => alive.has(pid));
    if (remaining.length === 0 || now() > deadline) return remaining;
    await sleep(200);
  }
}

async function systemEventsWindowsAllowed() {
  try {
    await execFileP("osascript", ["-e", 'tell application "System Events" to count windows of process "Finder"']);
    return true;
  } catch (err) {
    log(`System Events window queries not permitted (${(err.stderr || err.message).trim()}); using RSS fallback`);
    return false;
  }
}

async function queryWindowCount(pid) {
  const script = `tell application "System Events" to count windows of (first process whose unix id is ${pid})`;
  const started = now();
  try {
    const { stdout } = await execFileP("osascript", ["-e", script]);
    return { count: Number.parseInt(stdout.trim(), 10), error: null, latencyMs: now() - started };
  } catch (err) {
    // "Can't get process" (-1719) is expected until System Events registers the new process.
    return { count: null, error: (err.stderr || err.message).trim(), latencyMs: now() - started };
  }
}

/** Returns the time at which the first window was observed. */
async function waitForWindow(child, t0, queryLatencies, queryCounts) {
  const deadline = t0 + READY_TIMEOUT_MS;
  while (now() < deadline) {
    assertAlive(child);
    const { count, error, latencyMs } = await queryWindowCount(child.pid);
    queryLatencies.push(latencyMs);
    queryCounts.push(count);
    if (error !== null && PERMISSION_RE.test(error)) {
      throw new Error(`System Events denied window query: ${error}`);
    }
    if (count !== null && count >= 1) return now();
    await sleep(POLL_MS);
  }
  throw new Error(`no window appeared within ${READY_TIMEOUT_MS} ms`);
}

/** Returns when RSS became flat (stableSince) and when that flatness was confirmed (confirmedAt). */
async function waitForStableRss(child, t0, baseline) {
  const deadline = t0 + READY_TIMEOUT_MS;
  const history = [];
  while (now() < deadline) {
    assertAlive(child);
    const tree = appTree(await listProcesses(), child.pid, baseline);
    if (!tree.some((p) => p.pid === child.pid)) throw new Error("app process not found while measuring RSS");
    const t = now();
    history.push({ t, mb: sumMb(tree) });
    while (history.length > 1 && t - history[1].t >= FLAT_WINDOW_MS) history.shift();
    if (t - history[0].t >= FLAT_WINDOW_MS) {
      const values = history.map((h) => h.mb);
      const min = Math.min(...values);
      const max = Math.max(...values);
      const mean = values.reduce((a, b) => a + b, 0) / values.length;
      if (max - min <= FLAT_TOLERANCE * mean) return { stableSince: history[0].t, confirmedAt: t };
    }
    await sleep(POLL_MS);
  }
  throw new Error(`RSS did not stabilise within ${READY_TIMEOUT_MS} ms`);
}

/** Terminates the app and waits for its helpers to exit, so the next run starts cold. */
async function stopApp(child, baseline) {
  const helpers = appTree(await listProcesses(), child.pid, baseline)
    .map((p) => p.pid)
    .filter((pid) => pid !== child.pid);
  if (isAlive(child)) child.kill("SIGTERM");
  if (!(await waitExit(child, 5000))) {
    child.kill("SIGKILL");
    await waitExit(child, 2000);
  }
  let remaining = await waitGone(helpers, 10_000);
  for (const pid of remaining) {
    try {
      process.kill(pid, "SIGTERM");
    } catch {
      // already gone
    }
  }
  remaining = await waitGone(remaining, 3000);
  if (remaining.length > 0) log(`warning: ${remaining.length} WebKit helper(s) still alive after kill`);
}

async function runOnce(binary, mode, run) {
  const baseline = new Set(webkitPids(await listProcesses()));
  const queryLatencies = [];
  const queryCounts = [];
  const t0 = now();
  const child = spawn(binary, [], { stdio: "ignore" });
  child.once("error", (err) => log(`spawn error: ${err.message}`));
  activeChild = child;
  try {
    let readyAt;
    let confirmedAt;
    if (mode === "window") {
      confirmedAt = await waitForWindow(child, t0, queryLatencies, queryCounts);
      readyAt = confirmedAt;
    } else {
      ({ stableSince: readyAt, confirmedAt } = await waitForStableRss(child, t0, baseline));
    }
    const readyMs = readyAt - t0;

    const idleSamples = [];
    for (let i = 0; i < IDLE_SAMPLES; i++) {
      await sleep(Math.max(0, confirmedAt + IDLE_DELAY_MS + i * IDLE_STEP_MS - now()));
      const tree = appTree(await listProcesses(), child.pid, baseline);
      if (!tree.some((p) => p.pid === child.pid)) throw new Error("app process exited during idle sampling");
      idleSamples.push({
        mb: sumMb(tree),
        processes: tree.length,
        webkitProcesses: tree.filter((p) => WEBKIT_RE.test(p.command)).length,
      });
    }
    const idleRssMb = median(idleSamples.map((s) => s.mb));
    log(`run ${run}: ${readyMs.toFixed(0)} ms ready, ${idleRssMb.toFixed(1)} MB idle RSS`);

    return {
      run,
      readyMs: round1(readyMs),
      idleRssMb: round1(idleRssMb),
      idleSamplesMb: idleSamples.map((s) => round1(s.mb)),
      processesInTree: Math.max(...idleSamples.map((s) => s.processes)),
      webkitProcessesInTree: Math.max(...idleSamples.map((s) => s.webkitProcesses)),
      ...(mode === "window"
        ? {
            windowQueries: queryCounts.length,
            windowQueryMedianMs: round1(median(queryLatencies)),
          }
        : {}),
    };
  } finally {
    await stopApp(child, baseline);
    activeChild = null;
  }
}

async function main() {
  if (process.platform !== "darwin") {
    throw new Error("perf-desktop.mjs measures macOS only (System Events window queries, WebKit helpers)");
  }
  const { binary, runs } = parseArgs(process.argv.slice(2));
  const appPath = path.resolve(binary);
  await fs.access(appPath, fs.constants.X_OK);

  const mode = (await systemEventsWindowsAllowed()) ? "window" : "rss";
  const coldStartMetric = mode === "window" ? "spawn to first window" : "spawn to stable RSS";
  log(`${runs} run(s), cold-start metric: ${coldStartMetric}`);

  const results = [];
  const failures = [];
  for (let run = 1; run <= runs; run++) {
    try {
      results.push(await runOnce(appPath, mode, run));
    } catch (err) {
      failures.push({ run, error: err.message });
      log(`run ${run} failed: ${err.message}`);
    }
    await sleep(1000);
  }

  const macos = (await execFileP("sw_vers", ["-productVersion"])).stdout.trim();
  const coldMs = results.map((r) => r.readyMs);
  const idleMb = results.map((r) => r.idleRssMb);
  const coldStats = summarize(coldMs);
  const idleStats = summarize(idleMb);

  const report = {
    binary: appPath,
    host: { macos, arch: os.arch(), node: process.version },
    runsRequested: runs,
    runsCompleted: results.length,
    failures,
    coldStart: {
      metric: coldStartMetric,
      unit: "ms",
      samples: coldMs,
      median: coldStats.median,
      p95: coldStats.p95,
      budgetMs: COLD_START_BUDGET_MS,
      verdictStatistic: "p95",
      pass: results.length === 0 ? null : p95(coldMs) < COLD_START_BUDGET_MS,
    },
    idleRss: {
      metric: "process-tree RSS, median of 5 samples 10-14 s after ready",
      unit: "MB",
      samples: idleMb,
      median: idleStats.median,
      p95: idleStats.p95,
      budgetMb: IDLE_RSS_BUDGET_MB,
      verdictStatistic: "p95",
      pass: results.length === 0 ? null : p95(idleMb) < IDLE_RSS_BUDGET_MB,
    },
    runs: results,
  };

  console.log(JSON.stringify(report, null, 2));
  if (failures.length > 0) process.exitCode = 1;
}

main().catch((err) => {
  console.error(`[perf-desktop] ${err.message}`);
  process.exit(1);
});
