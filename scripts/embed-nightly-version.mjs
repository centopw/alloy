#!/usr/bin/env node
/**
 * Helper script to embed git describe and commit hash into Tauri version metadata.
 * Used during nightly builds.
 */
import fs from "node:fs";
import { execSync } from "node:child_process";

try {
  const describe = execSync("git describe --tags --always 2>/dev/null || echo dev").toString().trim();
  const commit = execSync("git rev-parse --short HEAD 2>/dev/null || echo head").toString().trim();
  const nightlyVersion = `0.1.0-nightly+${commit}.${describe}`;

  const configPath = "apps/alloy/src-tauri/tauri.conf.json";
  if (fs.existsSync(configPath)) {
    const conf = JSON.parse(fs.readFileSync(configPath, "utf8"));
    conf.version = nightlyVersion;
    fs.writeFileSync(configPath, JSON.stringify(conf, null, 2) + "\n");
    console.log(`Updated ${configPath} version to ${nightlyVersion}`);
  } else {
    console.error(`Config file not found: ${configPath}`);
    process.exit(1);
  }
} catch (err) {
  console.error("Failed to embed nightly version:", err);
  process.exit(1);
}
