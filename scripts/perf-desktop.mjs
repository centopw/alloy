#!/usr/bin/env node
/**
 * Cold start and idle memory benchmark for the release desktop app (macOS, Linux, Windows).
 * Budgets and method: docs/plan/08-delivery-p1.md sections 2 and 3, docs/CI.md.
 *
 * Usage:
 *   node scripts/perf-desktop.mjs <app-binary> [--runs N] [--report-only] [--json PATH]
 *                                 [--max-cold-start-ms N] [--max-idle-mb N] [--label TEXT]
 *
 * Per run:
 *   1. Spawn the binary and start a monotonic clock just before spawn.
 *   2. Wait for the app's first window:
 *      - macOS: poll System Events (about every 50 ms, bounded by osascript latency).
 *      - Linux: poll `xdotool search --onlyvisible --pid` on the current X display.
 *      - Windows: one PowerShell loop polling the process's MainWindowHandle.
 *      Metric: "spawn to first window". If window detection is unavailable (no display,
 *      missing tool, or denied automation permission), the whole benchmark falls back to
 *      "spawn to stable RSS": poll process-tree memory until it is flat (max-min <= 2% of
 *      mean) over >= 500 ms; the metric is the time at which that flat period began.
 *   3. Sample idle process-tree memory 10 s after ready: five samples at 1 s intervals.
 *      The tree is the app PID, its descendants, and WebView helpers started by this launch.
 *      Memory per process avoids double counting where the OS offers it:
 *      macOS `footprint` (physical footprint), Linux `/proc/<pid>/smaps_rollup` Pss,
 *      Windows `PageFileUsage` (private committed bytes). RSS is the fallback and is
 *      reported as such.
 *   4. Terminate the app, then wait for its helpers to exit before the next run.
 *
 * Exit status: 1 if a run failed, or if a budget is breached and the run is not
 * report-only. `--report-only` prints the same verdicts but always exits 0 unless a run
 * failed, which is how the first nightly run is wired.
 *
 * Progress goes to stderr. A JSON report goes to stdout (and to --json when given).
 */
import { execFile, spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const execFileP = promisify(execFile);
const PLATFORM = process.platform;

const DEFAULT_RUNS = 5;
const COLD_START_BUDGET_MS = 2000;
const IDLE_RSS_BUDGET_MB = 150;
const IDLE_DELAY_MS = 10_000;
const IDLE_SAMPLES = 5;
const IDLE_STEP_MS = 1000;
const WINDOW_POLL_MS = 50;
const LIST_POLL_MS = PLATFORM === "win32" ? 250 : 50;
const READY_TIMEOUT_MS = 60_000;
const FLAT_WINDOW_MS = 500;
const FLAT_TOLERANCE = 0.02;
const MAC_WEBKIT_RE = /WebKit\.framework|com\.apple\.WebKit\./;
const WINDOWS_WEBVIEW_RE = /msedgewebview2/i;
const PERMISSION_RE = /not authori[sz]ed|not allowed|-1743|-25211|assistive access/i;

let activeChild = null;
process.on("exit", () => {
  if (activeChild !== null && isAlive(activeChild)) {
    if (PLATFORM === "win32") {
      try {
        spawn("taskkill", ["/PID", String(activeChild.pid), "/T", "/F"], { stdio: "ignore" });
      } catch {
        // best effort
      }
    } else {
      activeChild.kill("SIGKILL");
    }
  }
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
  let reportOnly = false;
  let jsonPath = null;
  let maxColdStartMs = COLD_START_BUDGET_MS;
  let maxIdleMb = IDLE_RSS_BUDGET_MB;
  let label = null;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--runs") runs = Number(argv[++i]);
    else if (arg.startsWith("--runs=")) runs = Number(arg.slice("--runs=".length));
    else if (arg === "--report-only") reportOnly = true;
    else if (arg === "--json") jsonPath = argv[++i];
    else if (arg.startsWith("--json=")) jsonPath = arg.slice("--json=".length);
    else if (arg === "--max-cold-start-ms") maxColdStartMs = Number(argv[++i]);
    else if (arg.startsWith("--max-cold-start-ms=")) maxColdStartMs = Number(arg.slice("--max-cold-start-ms=".length));
    else if (arg === "--max-idle-mb") maxIdleMb = Number(argv[++i]);
    else if (arg.startsWith("--max-idle-mb=")) maxIdleMb = Number(arg.slice("--max-idle-mb=".length));
    else if (arg === "--label") label = argv[++i];
    else if (arg.startsWith("-")) throw new Error(`unknown option: ${arg}`);
    else if (binary === null) binary = arg;
    else throw new Error(`unexpected argument: ${arg}`);
  }
  if (binary === null) {
    throw new Error(
      "usage: node scripts/perf-desktop.mjs <app-binary> [--runs N] [--report-only] [--json PATH] [--max-cold-start-ms N] [--max-idle-mb N] [--label TEXT]",
    );
  }
  if (!Number.isInteger(runs) || runs < 1) throw new Error(`--runs must be a positive integer, got ${runs}`);
  if (!Number.isFinite(maxColdStartMs) || maxColdStartMs <= 0) throw new Error(`--max-cold-start-ms must be positive, got ${maxColdStartMs}`);
  if (!Number.isFinite(maxIdleMb) || maxIdleMb <= 0) throw new Error(`--max-idle-mb must be positive, got ${maxIdleMb}`);
  return { binary, runs, reportOnly, jsonPath, maxColdStartMs, maxIdleMb, label };
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

async function hasCommand(command, args = ["--version"]) {
  try {
    await execFileP(command, args, { timeout: 10_000 });
    return true;
  } catch {
    return false;
  }
}

async function listProcesses() {
  if (PLATFORM === "win32") return listProcessesWindows();
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

async function listProcessesWindows() {
  const script =
    "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,WorkingSetSize,PageFileUsage,Name | ConvertTo-Json -Compress";
  const { stdout } = await execFileP(
    "powershell",
    ["-NoProfile", "-NonInteractive", "-Command", script],
    { maxBuffer: 64 * 1024 * 1024 },
  );
  const trimmed = stdout.trim();
  const parsed = trimmed === "" ? [] : JSON.parse(trimmed);
  const rows = Array.isArray(parsed) ? parsed : [parsed];
  return rows.map((r) => ({
    pid: r.ProcessId,
    ppid: r.ParentProcessId,
    workingSetBytes: r.WorkingSetSize,
    privateBytes: r.PageFileUsage,
    command: r.Name ?? "",
  }));
}

/** PIDs of WebView helpers present in a process list (macOS XPC helpers and Windows WebView2). */
function helperPids(procs) {
  if (PLATFORM === "win32") {
    return procs.filter((p) => WINDOWS_WEBVIEW_RE.test(p.command)).map((p) => p.pid);
  }
  if (PLATFORM === "darwin") {
    return procs.filter((p) => MAC_WEBKIT_RE.test(p.command)).map((p) => p.pid);
  }
  return [];
}

/**
 * The app's process tree: the app PID, its descendants, and WebView helpers that were not
 * running before this launch. Pre-existing helpers (for example Safari's) are excluded
 * from the baseline.
 */
function appTree(procs, rootPid, helperBaseline) {
  const byPid = new Map(procs.map((p) => [p.pid, p]));
  const children = new Map();
  for (const p of procs) {
    const list = children.get(p.ppid);
    if (list) list.push(p.pid);
    else children.set(p.ppid, [p.pid]);
  }
  const seeds = [rootPid, ...helperPids(procs).filter((pid) => !helperBaseline.has(pid))];
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

/** Parses `footprint <pid>` output: "Footprint: 217 MB (16384 bytes per page)". */
function parseFootprintMb(stdout) {
  const match = /Footprint:\s*([\d.]+)\s*(bytes|KB|MB|GB)/.exec(stdout);
  if (match === null) return null;
  const value = Number(match[1]);
  switch (match[2]) {
    case "bytes":
      return value / (1024 * 1024);
    case "KB":
      return value / 1024;
    case "GB":
      return value * 1024;
    default:
      return value;
  }
}

/**
 * Memory of one process in MB, preferring a metric that does not double count pages
 * shared between processes. `method` records what was used.
 */
async function processMemoryMb(proc) {
  if (PLATFORM === "darwin") {
    try {
      const { stdout } = await execFileP("footprint", [String(proc.pid)], { maxBuffer: 4 * 1024 * 1024 });
      const mb = parseFootprintMb(stdout);
      if (mb !== null) return { mb, method: "footprint" };
    } catch {
      // fall through to RSS
    }
    return { mb: proc.rssKb / 1024, method: "rss" };
  }
  if (PLATFORM === "linux") {
    try {
      const text = await fs.readFile(`/proc/${proc.pid}/smaps_rollup`, "utf8");
      const match = /^Pss:\s+(\d+)\s+kB/m.exec(text);
      if (match !== null) return { mb: Number(match[1]) / 1024, method: "pss" };
    } catch {
      // fall through to RSS
    }
    return { mb: proc.rssKb / 1024, method: "rss" };
  }
  // Windows: PageFileUsage is the process's private committed bytes.
  if (typeof proc.privateBytes === "number" && proc.privateBytes > 0) {
    return { mb: proc.privateBytes / (1024 * 1024), method: "private" };
  }
  return { mb: (proc.workingSetBytes ?? 0) / (1024 * 1024), method: "working-set" };
}

/** Process-tree memory in MB plus the set of methods that produced it. */
async function treeMemory(tree) {
  const values = await Promise.all(tree.map((proc) => processMemoryMb(proc)));
  const totalMb = values.reduce((total, value) => total + value.mb, 0);
  const methods = [...new Set(values.map((value) => value.method))].sort();
  return { totalMb, method: methods.join("+") };
}

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

async function queryWindowCountDarwin(pid) {
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

/** Returns the time at which the first window was observed (macOS, System Events). */
async function waitForWindowDarwin(child, t0, diagnostics) {
  const deadline = t0 + READY_TIMEOUT_MS;
  while (now() < deadline) {
    assertAlive(child);
    const { count, error, latencyMs } = await queryWindowCountDarwin(child.pid);
    diagnostics.queryLatencies.push(latencyMs);
    if (error !== null && PERMISSION_RE.test(error)) {
      throw new Error(`System Events denied window query: ${error}`);
    }
    if (count !== null && count >= 1) return now();
    await sleep(WINDOW_POLL_MS);
  }
  throw new Error(`no window appeared within ${READY_TIMEOUT_MS} ms`);
}

async function xdotoolWindowIds(pid) {
  try {
    const { stdout } = await execFileP("xdotool", ["search", "--onlyvisible", "--pid", String(pid)]);
    return { ids: stdout.trim().split(/\s+/).filter(Boolean), error: null };
  } catch (err) {
    // xdotool exits 1 when no window matches; anything else is a real failure.
    if (err.code === 1) return { ids: [], error: null };
    return { ids: [], error: (err.stderr || err.message).trim() };
  }
}

/** Returns the time at which the first window was observed (Linux, X11 via xdotool). */
async function waitForWindowLinux(child, t0, diagnostics) {
  const deadline = t0 + READY_TIMEOUT_MS;
  while (now() < deadline) {
    assertAlive(child);
    const { ids, error } = await xdotoolWindowIds(child.pid);
    if (error !== null) throw new Error(`xdotool window query failed: ${error}`);
    diagnostics.windowQueries += 1;
    if (ids.length > 0) return now();
    await sleep(WINDOW_POLL_MS);
  }
  throw new Error(`no window appeared within ${READY_TIMEOUT_MS} ms`);
}

/** Returns the time at which the first window was observed (Windows, MainWindowHandle). */
async function waitForWindowWindows(child, t0, diagnostics) {
  const pid = child.pid;
  const script = `
$ErrorActionPreference = 'SilentlyContinue'
$sw = [Diagnostics.Stopwatch]::StartNew()
$result = 'TIMEOUT'
while ($sw.Elapsed.TotalMilliseconds -lt ${READY_TIMEOUT_MS}) {
  $p = Get-Process -Id ${pid}
  if ($null -eq $p) { $result = 'EXITED'; break }
  if ($p.MainWindowHandle -ne 0) { $result = "READY $([int]$sw.Elapsed.TotalMilliseconds)"; break }
  Start-Sleep -Milliseconds ${WINDOW_POLL_MS}
}
Write-Output $result
`;
  const started = now();
  const { stdout } = await execFileP(
    "powershell",
    ["-NoProfile", "-NonInteractive", "-Command", script],
    { timeout: READY_TIMEOUT_MS + 30_000 },
  );
  diagnostics.windowQueries += 1;
  const out = stdout.trim();
  if (out === "EXITED") {
    assertAlive(child);
    throw new Error("app exited before opening a window");
  }
  const match = /^READY (\d+)$/.exec(out);
  if (match === null) throw new Error(`no window appeared within ${READY_TIMEOUT_MS} ms`);
  return started + Number(match[1]);
}

/** Returns when memory became flat (stableSince) and when that flatness was confirmed (confirmedAt). */
async function waitForStableMemory(child, t0, helperBaseline) {
  const deadline = t0 + READY_TIMEOUT_MS;
  const history = [];
  while (now() < deadline) {
    assertAlive(child);
    const tree = appTree(await listProcesses(), child.pid, helperBaseline);
    if (!tree.some((p) => p.pid === child.pid)) throw new Error("app process not found while measuring memory");
    const { totalMb } = await treeMemory(tree);
    const t = now();
    history.push({ t, mb: totalMb });
    while (history.length > 1 && t - history[1].t >= FLAT_WINDOW_MS) history.shift();
    if (t - history[0].t >= FLAT_WINDOW_MS) {
      const values = history.map((h) => h.mb);
      const min = Math.min(...values);
      const max = Math.max(...values);
      const mean = values.reduce((a, b) => a + b, 0) / values.length;
      if (max - min <= FLAT_TOLERANCE * mean) return { stableSince: history[0].t, confirmedAt: t };
    }
    await sleep(LIST_POLL_MS);
  }
  throw new Error(`memory did not stabilise within ${READY_TIMEOUT_MS} ms`);
}

/** Terminates the app and waits for its helpers to exit, so the next run starts cold. */
async function stopApp(child, helperBaseline) {
  if (PLATFORM === "win32") {
    try {
      await execFileP("taskkill", ["/PID", String(child.pid), "/T", "/F"]);
    } catch {
      // already gone
    }
    if (!(await waitExit(child, 5000))) child.kill("SIGKILL");
    await waitGone([child.pid], 5000);
    return;
  }
  const helpers = appTree(await listProcesses(), child.pid, helperBaseline)
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
  if (remaining.length > 0) log(`warning: ${remaining.length} helper process(es) still alive after kill`);
}

async function chooseMode() {
  if (PLATFORM === "darwin") {
    return (await systemEventsWindowsAllowed()) ? "window-darwin" : "rss";
  }
  if (PLATFORM === "linux") {
    if (!process.env.DISPLAY) {
      log("DISPLAY is not set (headless host?); using RSS fallback. Run under xvfb-run for window timing.");
      return "rss";
    }
    if (!(await hasCommand("xdotool", ["version"]))) {
      log("xdotool is not installed; using RSS fallback. Install xdotool for window timing.");
      return "rss";
    }
    return "window-linux";
  }
  if (PLATFORM === "win32") return "window-win32";
  throw new Error(`unsupported platform: ${PLATFORM}`);
}

async function runOnce(binary, mode, run) {
  const helperBaseline = new Set(helperPids(await listProcesses()));
  const diagnostics = { queryLatencies: [], windowQueries: 0 };
  const t0 = now();
  const child = spawn(binary, [], { stdio: "ignore" });
  child.once("error", (err) => log(`spawn error: ${err.message}`));
  activeChild = child;
  try {
    let readyAt;
    let confirmedAt;
    if (mode === "window-darwin") {
      confirmedAt = await waitForWindowDarwin(child, t0, diagnostics);
      readyAt = confirmedAt;
    } else if (mode === "window-linux") {
      confirmedAt = await waitForWindowLinux(child, t0, diagnostics);
      readyAt = confirmedAt;
    } else if (mode === "window-win32") {
      confirmedAt = await waitForWindowWindows(child, t0, diagnostics);
      readyAt = confirmedAt;
    } else {
      ({ stableSince: readyAt, confirmedAt } = await waitForStableMemory(child, t0, helperBaseline));
    }
    const readyMs = readyAt - t0;

    const idleSamples = [];
    for (let i = 0; i < IDLE_SAMPLES; i++) {
      await sleep(Math.max(0, confirmedAt + IDLE_DELAY_MS + i * IDLE_STEP_MS - now()));
      const tree = appTree(await listProcesses(), child.pid, helperBaseline);
      if (!tree.some((p) => p.pid === child.pid)) throw new Error("app process exited during idle sampling");
      const memory = await treeMemory(tree);
      idleSamples.push({
        mb: memory.totalMb,
        method: memory.method,
        processes: tree.length,
        webviewProcesses: helperPids(tree).length,
      });
    }
    const idleRssMb = median(idleSamples.map((s) => s.mb));
    log(`run ${run}: ${readyMs.toFixed(0)} ms ready, ${idleRssMb.toFixed(1)} MB idle`);

    return {
      run,
      readyMs: round1(readyMs),
      idleRssMb: round1(idleRssMb),
      idleSamplesMb: idleSamples.map((s) => round1(s.mb)),
      memoryMethod: [...new Set(idleSamples.map((s) => s.method))].join("+"),
      processesInTree: Math.max(...idleSamples.map((s) => s.processes)),
      webviewProcessesInTree: Math.max(...idleSamples.map((s) => s.webviewProcesses)),
      ...(mode === "window-darwin"
        ? {
            windowQueries: diagnostics.queryLatencies.length,
            windowQueryMedianMs: round1(median(diagnostics.queryLatencies)),
          }
        : {}),
      ...(mode === "window-linux" || mode === "window-win32" ? { windowQueries: diagnostics.windowQueries } : {}),
    };
  } finally {
    await stopApp(child, helperBaseline);
    activeChild = null;
  }
}

async function hostInfo() {
  const host = { platform: PLATFORM, release: os.release(), arch: os.arch(), node: process.version, cpus: os.cpus().length };
  try {
    if (PLATFORM === "darwin") {
      host.osVersion = (await execFileP("sw_vers", ["-productVersion"])).stdout.trim();
    } else if (PLATFORM === "linux") {
      const release = await fs.readFile("/etc/os-release", "utf8");
      const match = /^PRETTY_NAME="?([^"\n]+)"?$/m.exec(release);
      if (match !== null) host.osVersion = match[1];
    } else {
      const { stdout } = await execFileP(
        "powershell",
        ["-NoProfile", "-NonInteractive", "-Command", "(Get-CimInstance Win32_OperatingSystem).Caption"],
      );
      host.osVersion = stdout.trim();
    }
  } catch {
    // host version is informational only
  }
  return host;
}

function annotate(message, isError) {
  if (process.env.GITHUB_ACTIONS === "true") {
    console.error(`::${isError ? "error" : "warning"} title=perf budget::${message.replace(/\n/g, "%0A")}`);
  }
}

async function main() {
  const { binary, runs, reportOnly, jsonPath, maxColdStartMs, maxIdleMb, label } = parseArgs(process.argv.slice(2));
  const appPath = path.resolve(binary);
  await fs.access(appPath, fs.constants.X_OK);

  const mode = await chooseMode();
  const windowMode = mode !== "rss";
  const coldStartMetric = windowMode ? "spawn to first window" : "spawn to stable memory";
  log(`${runs} run(s) on ${PLATFORM}, cold-start metric: ${coldStartMetric}, gate: ${reportOnly ? "report-only" : "enforced"}`);

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

  const coldMs = results.map((r) => r.readyMs);
  const idleMb = results.map((r) => r.idleRssMb);
  const coldStats = summarize(coldMs);
  const idleStats = summarize(idleMb);
  const coldPass = windowMode && results.length > 0 ? p95(coldMs) < maxColdStartMs : null;
  const idlePass = results.length > 0 ? p95(idleMb) < maxIdleMb : null;
  const breaches = [];
  if (coldPass === false) breaches.push(`cold start p95 ${coldStats.p95} ms >= ${maxColdStartMs} ms`);
  if (idlePass === false) breaches.push(`idle memory p95 ${idleStats.p95} MB >= ${maxIdleMb} MB`);

  const report = {
    schemaVersion: 2,
    label,
    binary: appPath,
    host: await hostInfo(),
    mode,
    gate: { reportOnly, enforced: !reportOnly, breaches, exitCode: null },
    runsRequested: runs,
    runsCompleted: results.length,
    failures,
    coldStart: {
      metric: coldStartMetric,
      unit: "ms",
      samples: coldMs,
      median: coldStats.median,
      p95: coldStats.p95,
      budgetMs: windowMode ? maxColdStartMs : null,
      verdictStatistic: "p95",
      pass: coldPass,
    },
    idleMemory: {
      metric: `${results[0]?.memoryMethod ?? "unknown"} process-tree memory, median of 5 samples 10-14 s after ready`,
      unit: "MB",
      samples: idleMb,
      median: idleStats.median,
      p95: idleStats.p95,
      budgetMb: maxIdleMb,
      verdictStatistic: "p95",
      pass: idlePass,
    },
    runs: results,
  };

  if (breaches.length > 0) {
    const verdict = `${breaches.join("; ")}${reportOnly ? " (report-only: not failing this run)" : ""}`;
    annotate(verdict, !reportOnly);
    log(`budget breach: ${verdict}`);
  }
  const failedByGate = !reportOnly && breaches.length > 0;
  report.gate.exitCode = failures.length > 0 || failedByGate ? 1 : 0;

  const json = JSON.stringify(report, null, 2);
  if (jsonPath !== null) {
    await fs.mkdir(path.dirname(path.resolve(jsonPath)), { recursive: true });
    await fs.writeFile(path.resolve(jsonPath), json + "\n");
    log(`report written to ${path.resolve(jsonPath)}`);
  }
  console.log(json);
  if (failures.length > 0 || failedByGate) process.exitCode = 1;
}

main().catch((err) => {
  console.error(`[perf-desktop] ${err.message}`);
  process.exit(1);
});
