#!/usr/bin/env node
/**
 * Alloy Timeline Scroll Performance Measurement on Android WebView
 * 
 * Uses direct Chrome DevTools Protocol over WebSocket to measure
 * Frame Time and FPS during rapid scrolling of the virtualized 10,000-item timeline
 * running inside the Android Emulator WebView.
 */

import http from "node:http";

console.log("🚀 Alloy Timeline Android WebView Perf Measurement");
console.log("=".repeat(60));

async function getPageTarget() {
  const res = await fetch("http://127.0.0.1:9222/json");
  const targets = await res.json();
  const pageTarget = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl) || targets[0];
  if (!pageTarget || !pageTarget.webSocketDebuggerUrl) {
    throw new Error("No page target found. Ensure adb forward tcp:9222 is active and app is open.");
  }
  return pageTarget;
}

function runCdpEvaluation(wsUrl, code) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    let msgId = 1;

    ws.onopen = () => {
      console.log(`✓ Connected to WebView via WebSocket: ${wsUrl}`);
      ws.send(JSON.stringify({
        id: msgId++,
        method: "Runtime.evaluate",
        params: {
          expression: code,
          awaitPromise: true,
          returnByValue: true,
        },
      }));
    };

    ws.onmessage = (event) => {
      const data = JSON.parse(event.data);
      if (data.id === 1) {
        ws.close();
        if (data.error) {
          reject(new Error(JSON.stringify(data.error)));
        } else if (data.result && data.result.exceptionDetails) {
          reject(new Error(JSON.stringify(data.result.exceptionDetails)));
        } else {
          resolve(data.result.result.value);
        }
      }
    };

    ws.onerror = (err) => {
      reject(err);
    };
  });
}

async function measure() {
  try {
    const pageTarget = await getPageTarget();
    console.log(`✓ Target page: ${pageTarget.title} (${pageTarget.url})`);

    console.log("⏳ Injecting scroll benchmark into Android WebView...");
    const benchmarkCode = `
      (async function() {
        return new Promise((resolve) => {
          const frameTimes = [];
          let lastTime = performance.now();
          let scrollInterval = null;
          let frameCount = 0;
          const recorder = () => {
            const now = performance.now();
            if (frameCount > 0) {
              const delta = now - lastTime;
              frameTimes.push(delta);
            }
            lastTime = now;
            frameCount++;

            // Collect 1500+ frames or until 30 seconds pass
            if (frameCount >= 1500) {
              clearInterval(scrollInterval);
              resolve(frameTimes);
            } else {
              requestAnimationFrame(recorder);
            }
          };

          // Continuous programmatic scrolling down the virtualized list
          const viewport = document.querySelector(".timeline-viewport");
          scrollInterval = setInterval(() => {
            if (viewport) {
              viewport.scrollTop += 50;
            }
          }, 16);

          // Automatically stopped when frameCount >= 1500
          requestAnimationFrame(recorder);
        });
      })()
    `;

    console.log("📊 Recording 1000 frame times during active scrolling...");
    const frameTimes = await runCdpEvaluation(pageTarget.webSocketDebuggerUrl, benchmarkCode);
    console.log(`✓ Captured ${frameTimes.length} frames from Android WebView!`);

    const sorted = [...frameTimes].sort((a, b) => a - b);
    const avg = frameTimes.reduce((a, b) => a + b, 0) / frameTimes.length;
    const p50 = sorted[Math.floor(sorted.length * 0.50)];
    const p90 = sorted[Math.floor(sorted.length * 0.90)];
    const p95 = sorted[Math.floor(sorted.length * 0.95)];
    const p99 = sorted[Math.floor(sorted.length * 0.99)];
    const max = sorted[sorted.length - 1];

    const fps = Math.round(1000 / avg);
    const droppedFrames60 = frameTimes.filter((t) => t > 16.6).length;
    const droppedFrames48 = frameTimes.filter((t) => t > 20.8).length;

    const results = {
      timestamp: new Date().toISOString(),
      environment: {
        platform: "Android 16 arm64 (Emulator: HCE/Medium_Phone)",
        browser: "Chrome 153.0.8010.36 WebView",
        target: "chat.alloy",
      },
      measurements: {
        frameCount: frameTimes.length,
        avgFps: fps,
        avgFrameTime_ms: Math.round(avg * 100) / 100,
        p50_ms: Math.round(p50 * 100) / 100,
        p90_ms: Math.round(p90 * 100) / 100,
        p95_ms: Math.round(p95 * 100) / 100,
        p99_ms: Math.round(p99 * 100) / 100,
        maxFrameTime_ms: Math.round(max * 100) / 100,
      },
      qualityMetrics: {
        droppedFrames_over16ms: droppedFrames60,
        droppedFrames_over20ms: droppedFrames48,
        percentageSmooth: Math.round(((frameTimes.length - droppedFrames60) / frameTimes.length) * 100),
      },
      disclaimer: "Emulator numbers are indicative only. Real low-end hardware is still required for Gate G1 final.",
    };

    console.log("\n" + "=".repeat(60));
    console.log("📈 ANDROID EMULATOR WEBVIEW SCROLL METRICS");
    console.log("=".repeat(60));
    console.log(`   Avg FPS: ${results.measurements.avgFps}`);
    console.log(`   Avg Frame Time: ${results.measurements.avgFrameTime_ms} ms`);
    console.log(`   p50 (median): ${results.measurements.p50_ms} ms`);
    console.log(`   p90: ${results.measurements.p90_ms} ms`);
    console.log(`   p95: ${results.measurements.p95_ms} ms`);
    console.log(`   p99: ${results.measurements.p99_ms} ms`);
    console.log(`   Max: ${results.measurements.maxFrameTime_ms} ms`);
    console.log(`   Dropped (>16.6ms): ${results.qualityMetrics.droppedFrames_over16ms}`);
    console.log(`   Dropped (>20.8ms): ${results.qualityMetrics.droppedFrames_over20ms}`);
    console.log(`   Smoothness: ${results.qualityMetrics.percentageSmooth}%`);
    console.log(`\n📢 ${results.disclaimer}`);
    console.log("=".repeat(60) + "\n");

    return results;
  } catch (err) {
    console.error("❌ Measurement error:", err);
    throw err;
  }
}

measure().catch(() => process.exit(1));
