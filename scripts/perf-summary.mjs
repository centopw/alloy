#!/usr/bin/env node
/**
 * Renders a perf-desktop JSON report as a Markdown summary (for $GITHUB_STEP_SUMMARY).
 * Usage: node scripts/perf-summary.mjs <report.json>
 */
import fs from "node:fs";

const reportPath = process.argv[2];
if (!reportPath) {
  console.error("usage: node scripts/perf-summary.mjs <report.json>");
  process.exit(2);
}

const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
const cold = report.coldStart;
const idle = report.idleMemory;
const show = (value) => (value === null || value === undefined ? "n/a" : value);
const verdict = (pass) => (pass === null || pass === undefined ? "not gated" : pass ? "pass" : "BREACH");

const lines = [
  `### Perf ${report.host.platform} ${report.host.arch} (${report.mode}, ${report.gate.reportOnly ? "report-only" : "enforced"})`,
  "",
  "| metric | median | p95 | budget | verdict |",
  "|---|---|---|---|---|",
  `| ${cold.metric} | ${show(cold.median)} ms | ${show(cold.p95)} ms | ${show(cold.budgetMs)} ms | ${verdict(cold.pass)} |`,
  `| ${idle.metric} | ${show(idle.median)} MB | ${show(idle.p95)} MB | ${idle.budgetMb} MB | ${verdict(idle.pass)} |`,
  `| runs completed | ${report.runsCompleted}/${report.runsRequested} | | | |`,
  "",
];

if (report.failures.length > 0) {
  lines.push(`Failed runs: ${report.failures.map((f) => `#${f.run} ${f.error}`).join("; ")}`, "");
}

process.stdout.write(lines.join("\n"));
