# 08: P1 Delivery, Release Pipelines & Performance Budgets

**Status:** Approved  
**Phase:** P1 (Desktop MVP v0.1 + Android Foundation)  
**Last Updated:** 2026-10-16  

---

## 1. Overview & Objectives

This document establishes the delivery architecture, release pipelines, and performance budgets for Alloy Phase 1 (P1). 

The goal of P1 delivery is twofold:
1. Provide reliable, automated nightly and stable release pipelines across all target tier-1 platforms (macOS arm64/x64, Linux, Windows, and Android).
2. Enforce strict performance budgets established in `docs/plan/04-security-delivery.md` to prevent regressions in cold-start latency, room-list rendering, virtualized timeline smoothness, and binary footprint.

---

## 2. P1 Performance Budget Table

Adopted from `docs/plan/04-security-delivery.md` and calibrated against the Phase 0 spikes (`docs/spikes/android.md`):

| Metric | Target Ceiling | Measurement Tooling | Enforcement Level | Frequency / Location |
|---|---|---|---|---|
| **Cold Start** | **<2.0 s** (Desktop)<br>**<3.5 s** (Mobile / Low-End) | Native launch timing (`am start -W` on Android; `samply` / monotonic clock on desktop) | Automated Monitoring | Nightly CI benchmark job + Release gate |
| **Time-to-First-Room-List** | **<500 ms** (after initial sync) | `criterion` benchmark suite in `alloy-core` | Automated CI Gate | Every PR on `main` (blocks on >5% regression) |
| **Timeline Frame Time (Scroll)** | **≥60 FPS** (median $\le 16.7$ ms)<br>**p95 $\le 21.7$ ms**<br>**<6% dropped frames** (>20.8 ms) at 10,000 events | Headless Chrome CDP (`measure-scroll-perf.mjs`) & Android CDP runner (`measure-scroll-perf-android.mjs`) | Automated & Manual | `just perf-ui` (pre-merge) + Weekly automated benchmark |
| **Idle Memory Footprint** | **<150 MB** (Desktop)<br>**<80 MB** (Mobile) | System process monitor (`ps`, `massif`, Android dumpsys meminfo) | Automated Monitoring | Nightly benchmark run |
| **Binary Size** | **<60 MB** (macOS DMG)<br>**<40 MB** (Android APK)<br>**<75 MB** (Windows Setup) | Build artifact size check (`ls -l`, `cargo-bloat`) | Release Checklist | Release workflow (`release.yml`) |

### 2.1 Measured so far (2026-10-17, nightly run 39040208030)

| Metric | Result | Verdict |
|---|---|---|
| Installer size | macOS arm64 DMG 29 MB, macOS x64 DMG 31 MB, Windows setup 18 MB (MSI 30 MB), Linux deb 34 MB / rpm 34 MB / AppImage 105 MB | within budget (AppImage has no budget; it bundles webkit) |
| Timeline scroll, 10k events, Android emulator WebView | p50 16.7 ms, p95 20-22 ms, p99 42-49 ms (docs/spikes/android.md) | indicative; real low-end device still required |
| Cold start, idle memory, time-to-first-room-list | not measured on release builds yet | open: tracked as a perf issue |

---

## 3. Metric Measurement Methodology

### 3.1 Cold Start Time
- **Definition:** Elapsed time between process execution start (`execve` / activity launch) and the initial interactive layout render.
- **Desktop Measurement:**
  ```bash
  samply record cargo run --release -p alloy-app
  ```
  Instruments first paint timestamp minus process startup monotonic timestamp.
- **Android Measurement:**
  ```bash
  adb shell am start-activity -W -n chat.alloy/chat.alloy.MainActivity -S
  ```
  Captures `TotalTime` reported by the Android Activity Manager.
  - *Observed Baseline (Phase 0 Spike):* 874 ms (Pixel-class warm), 1,844 ms (fresh wipe cold boot), 1,119 ms (Low-End 720p).

### 3.2 Time-to-First-Room-List
- **Definition:** Latency from the completion of the `/sync` response payload processing to the emission of the first materialized room-list view model to the UI.
- **Measurement Tooling:** `criterion` micro-benchmarks targeting `alloy-core` sliding-sync handlers.
- **Command:**
  ```bash
  cargo bench -p alloy-core --bench room_list_sync
  ```
- **CI Assertion:** Criterion compares result against the previous baseline; halts execution if regression exceeds 5.0%.

### 3.3 Timeline Frame Time (Continuous Scrolling)
- **Definition:** Individual frame rendering duration during active, continuous scrolling across 10,000 heterogeneous mock timeline items (text, images, notices, emotes).
- **Measurement Tooling:**
  - `apps/alloy/ui/scripts/measure-scroll-perf.mjs` (Puppeteer + Chrome DevTools Protocol).
  - Collects $\ge 1,500$ frames under active scroll.
- **Command:**
  ```bash
  just perf-ui
  ```
- **Evaluation Criteria:**
  - Average FPS: $\ge 55$ FPS.
  - p50 (Median): $\le 16.7$ ms (60 FPS display synchronization).
  - p95: $\le 21.7$ ms.
  - Dropped frames (>20.8 ms / <48 FPS): $<6\%$.

### 3.4 Binary Size
- **Definition:** Total size on disk of compiled, compressed installer and application packages.
- **Measurement:**
  ```bash
  cargo bloat --release --crates
  ls -lh apps/alloy/src-tauri/target/release/bundle/
  ```
- **CI Gate:** Evaluated in release pipelines; warns if binary approaches 90% of budget ceiling.

---

## 4. Enforcement Strategy & CI Integration

### Automated CI Pipeline (`.github/workflows/ci.yml`)
- **Unit & Integration Tests:** `cargo test --workspace --locked` and `synapse-integration` job run on every PR.
- **Contract Verification:**
  - `protocol-ts-check`: Fails if generated TypeScript bindings deviate from Rust protocol DTOs.
  - `theme-css-check`: Fails if generated CSS token stylesheets are out of sync with theme definitions.
- **Linting & Hygiene:** Clippy zero-warnings policy (`-D warnings`) and rustfmt check.

### Nightly Pipeline (`.github/workflows/nightly.yml`)
- Scheduled execution daily at 02:00 UTC.
- Builds all desktop bundles (macOS arm64 + x64, Windows, Linux) and Android debug APK.
- Validates end-to-end packaging and toolchain health independently of release tags.
- Publishes rolling `nightly` prerelease with embedded `git describe` and commit metadata.

### Release Pipeline (`.github/workflows/release.yml`)
- Runs on tag push `v*`.
- Enforces full cryptographic signing (Apple Developer ID + notarization, Windows Authenticode, Android release keystore, minisign updater signatures).
- Generates machine-readable CycloneDX SBOM (`alloy-sbom-*.json`) and `SHA256SUMS`.
