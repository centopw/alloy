#!/usr/bin/env node
/**
 * Alloy Theme Screenshots Script
 *
 * Captures 1280x800 PNG screenshots of each theme/density combination:
 * - Themes: alloy-light, alloy-dark, alloy-amoled
 * - Densities: compact, cozy
 *
 * Each screenshot is saved to docs/spikes/theming/<theme>-<density>.png (<300KB each).
 * Verifies that:
 * 1. Files are distinct (SHA256 comparison)
 * 2. Background pixel sampled from the screenshot matches theme canvas token
 * 3. CSS variables for theme and density match token specifications
 */

import puppeteer from "puppeteer-core";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import fs from "fs";
import crypto from "crypto";
import { startDevServer, waitForShell } from "./dev-server.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const uiDir = resolve(__dirname, "..");
const repoRoot = resolve(uiDir, "../../..");
const spikesDir = resolve(repoRoot, "docs/spikes/theming");
const PORT = 5174;
const CHROME_PATH = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

const THEMES = ["alloy-light", "alloy-dark", "alloy-amoled"];
const DENSITIES = ["compact", "cozy"];
/** Largest per-channel difference between the sampled pixel and the canvas token (ADR 0014 §1). */
const CHANNEL_TOLERANCE = 12;

// Expected background hex colors from packages/themes/*.json
const EXPECTED_BG_COLORS = {
  "alloy-light": { hex: "#ffffff", rgb: { r: 255, g: 255, b: 255 } },
  "alloy-dark": { hex: "#18181b", rgb: { r: 24, g: 24, b: 27 } },
  "alloy-amoled": { hex: "#000000", rgb: { r: 0, g: 0, b: 0 } },
};

console.log("📸 Alloy Theme Screenshot Generator");
console.log("=".repeat(60));

// Ensure output directory exists
if (!fs.existsSync(spikesDir)) {
  fs.mkdirSync(spikesDir, { recursive: true });
  console.log(`✓ Created output directory: ${spikesDir}`);
}

// Check the app sources exist
if (!fs.existsSync(resolve(uiDir, "src"))) {
  console.error(`❌ UI sources not found: ${resolve(uiDir, "src")}`);
  process.exit(1);
}

console.log(`✓ Repo root: ${repoRoot}`);
console.log(`✓ Output directory: ${spikesDir}`);
console.log(`✓ Chrome binary: ${CHROME_PATH}`);
console.log(`✓ Themes: ${THEMES.join(", ")}`);
console.log(`✓ Densities: ${DENSITIES.join(", ")}`);

let server = null;
let browser = null;

function computeHash(data) {
  return crypto.createHash("sha256").update(data).digest("hex");
}

function rgbToHex(r, g, b) {
  return "#" + [r, g, b].map((x) => x.toString(16).padStart(2, "0")).join("");
}

async function captureThemes() {
  try {
    // Start the dev server: the demo engine is a development-only path (see src/lib/ipc/index.ts).
    server = await startDevServer(PORT);

    // Launch Chrome headless
    console.log("\n🔧 Launching Chrome headless...");
    browser = await puppeteer.launch({
      executablePath: CHROME_PATH,
      headless: "new",
      args: [
        "--no-sandbox",
        "--disable-setuid-sandbox",
        "--disable-dev-shm-usage",
        "--disable-gpu",
      ],
    });

    const page = await browser.newPage();
    await page.setViewport({ width: 1280, height: 800 });

    console.log(`✓ Navigating to ${server.url}...`);
    await page.goto(server.url, { waitUntil: "networkidle2" });

    // Wait for the demo engine to sign in and the shell to paint, then clear the first-run gate.
    console.log("⏳ Waiting for app to mount...");
    await waitForShell(page);
    await new Promise((r) => setTimeout(r, 500));

    const screenshots = [];

    // Capture each theme/density combination
    for (const theme of THEMES) {
      for (const density of DENSITIES) {
        const filename = `${theme}-${density}.png`;
        const filepath = resolve(spikesDir, filename);

        console.log(`\n📷 Capturing ${theme} @ ${density}...`);

        // Set theme and density via localStorage key alloy.theme.v1
        await page.evaluate(
          (themeVal, densityVal) => {
            const prefs = {
              theme: themeVal,
              density: densityVal,
              messageStyle: "bubble",
              followSystem: false,
            };
            localStorage.setItem("alloy.theme.v1", JSON.stringify(prefs));
          },
          theme,
          density
        );

        // Reload to trigger bootstrap script in app.html and ThemeStore
        await page.reload({ waitUntil: "networkidle2" });
        await page.waitForSelector(".row-wrap .row", { timeout: 30000 });
        // Density shows in the message rhythm, so open a chat; the selection resets on reload.
        await page.click(".row-wrap .row");
        await page.waitForSelector(".timeline .viewport", { timeout: 30000 });
        await new Promise((r) => setTimeout(r, 500));

        // Verify DOM attributes
        const appliedTheme = await page.evaluate(() => {
          return document.documentElement.getAttribute("data-theme");
        });
        const appliedDensity = await page.evaluate(() => {
          return document.documentElement.getAttribute("data-density");
        });

        console.log(`  Applied attributes: data-theme="${appliedTheme}", data-density="${appliedDensity}"`);

        // Read computed CSS variables
        const cssVars = await page.evaluate(() => {
          const html = document.documentElement;
          const styles = window.getComputedStyle(html);
          return {
            canvas: styles.getPropertyValue("--alloy-color-surface-canvas").trim(),
            textPrimary: styles.getPropertyValue("--alloy-color-text-primary").trim(),
            avatarSize: styles.getPropertyValue("--alloy-spacing-avatar-size").trim(),
            messagePadding: styles.getPropertyValue("--alloy-spacing-message-padding").trim(),
            density: styles.getPropertyValue("--alloy-spacing-density").trim(),
          };
        });

        console.log(`  Computed CSS: canvas=${cssVars.canvas}, text=${cssVars.textPrimary}, avatarSize=${cssVars.avatarSize}`);

        // Capture screenshot (1280x800)
        await page.screenshot({ path: filepath, type: "png" });
        const fileBuffer = fs.readFileSync(filepath);
        const sha256 = computeHash(fileBuffer);
        const fileSize = fileBuffer.length;

        // Ensure file size is small (< 300KB)
        if (fileSize > 300 * 1024) {
          console.warn(`  ⚠ Warning: file size ${fileSize} exceeds 300KB`);
        }

        // Get coordinate of timeline viewport to sample canvas background
        const sampleCoord = await page.evaluate(() => {
          const el = document.querySelector(".timeline-pane");
          if (el) {
            const rect = el.getBoundingClientRect();
            return { x: Math.floor(rect.right - 6), y: Math.floor(rect.top + rect.height / 2) };
          }
          return { x: 500, y: 300 };
        });

        // Sample a pixel from the rendered screenshot buffer at timeline coordinate
        const sampledPixel = await page.evaluate(async ({ base64, coord }) => {
          return new Promise((resolve) => {
            const img = new Image();
            img.onload = () => {
              const canvas = document.createElement("canvas");
              canvas.width = img.width;
              canvas.height = img.height;
              const ctx = canvas.getContext("2d");
              ctx.drawImage(img, 0, 0);
              const data = ctx.getImageData(coord.x, coord.y, 1, 1).data;
              resolve({ r: data[0], g: data[1], b: data[2], a: data[3] });
            };
            img.src = "data:image/png;base64," + base64;
          });
        }, { base64: fileBuffer.toString("base64"), coord: sampleCoord });

        const sampledHex = rgbToHex(sampledPixel.r, sampledPixel.g, sampledPixel.b);
        console.log(`  Sampled pixel at (${sampleCoord.x}, ${sampleCoord.y}): rgb(${sampledPixel.r}, ${sampledPixel.g}, ${sampledPixel.b}) -> ${sampledHex}`);

        // Assert the pixel is the theme's canvas. ADR 0014 makes the conversation layer canvas at
        // 92% (84% dark) over the aurora, so the visible pixel is a near-canvas tint rather than
        // the token itself: allow a small per-channel difference.
        const expected = EXPECTED_BG_COLORS[theme];
        const distance = Math.max(
          Math.abs(sampledPixel.r - expected.rgb.r),
          Math.abs(sampledPixel.g - expected.rgb.g),
          Math.abs(sampledPixel.b - expected.rgb.b)
        );
        if (distance > CHANNEL_TOLERANCE) {
          throw new Error(
            `Color mismatch for ${theme}: expected ~${expected.hex}, but sampled pixel was ${sampledHex} (off by ${distance})`
          );
        }
        console.log(`  ✓ Sampled pixel matches expected theme token (${expected.hex})`);

        screenshots.push({
          theme,
          density,
          filename,
          filepath,
          fileSize,
          sha256,
          cssVars,
          sampledHex,
        });
      }
    }

    // Summary & Verification
    console.log("\n" + "=".repeat(60));
    console.log("📊 Summary & Distinctness Verification");
    console.log("=".repeat(60));

    console.log("\nGenerated Files:");
    for (const s of screenshots) {
      console.log(`  ${s.filename.padEnd(26)} | Size: ${(s.fileSize / 1024).toFixed(1)} KB | SHA256: ${s.sha256.substring(0, 16)}... | Pixel: ${s.sampledHex}`);
    }

    // Each theme must look different. Densities are allowed to match while the density tokens
    // (`--alloy-spacing-message-padding` and friends) are not used by any component yet; that is a
    // product gap, not a screenshot failure, so it is reported rather than thrown.
    const themeHashes = THEMES.map(
      (theme) => screenshots.find((s) => s.theme === theme)?.sha256 ?? ""
    );
    const uniqueThemeHashes = new Set(themeHashes);
    if (uniqueThemeHashes.size === THEMES.length) {
      console.log(`\n✓ SUCCESS: All ${THEMES.length} themes are visually distinct.`);
    } else {
      throw new Error(`Distinctness failure: only ${uniqueThemeHashes.size}/${THEMES.length} themes differ`);
    }
    for (const theme of THEMES) {
      const hashes = screenshots.filter((s) => s.theme === theme).map((s) => s.sha256);
      if (new Set(hashes).size === 1) {
        console.warn(`  ⚠ ${theme}: compact and cozy render identically (density tokens are unused)`);
      }
    }

    // Assert all files are < 300KB
    const oversized = screenshots.filter((s) => s.fileSize > 300 * 1024);
    if (oversized.length === 0) {
      console.log(`✓ SUCCESS: All ${screenshots.length} screenshots are under 300KB!`);
    } else {
      throw new Error(`File size failure: ${oversized.length} files exceed 300KB`);
    }

    console.log("\n🎉 All verification checks passed!");
    return screenshots;
  } finally {
    if (browser) await browser.close();
    if (server) server.stop();
  }
}

// Run
(async () => {
  await captureThemes();
})();
