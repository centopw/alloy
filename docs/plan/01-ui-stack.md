---
id: plan-01-ui-stack
type: architecture-design
status: draft
title: "Alloy Architecture Plan: UI Stack & Cross-Platform Shell Evaluation"
parent: alloy-architecture
tags: [architecture, ui, plan]
---

# Alloy Architecture Plan: UI Stack & Cross-Platform Shell Evaluation

- **Target Path**: `docs/plan/01-ui-stack.md`
- **Date**: 2026-10-16
- **Status**: Complete / Proposed Baseline

---

## 1. Summary & Recommendation

### Conclusion First
We recommend **Tauri 2 (Rust Core + Svelte 5 / TypeScript UI Shell)** as the primary UI architecture for Alloy, with **Flutter + `matrix-rust-sdk` (via FFI / `flutter_rust_bridge`)** as the designated fallback.

```
+---------------------------------------------------------------------------------------+
|                                    ALLOY CLIENT                                       |
+---------------------------------------------------------------------------------------+
|  UI Shell (Desktop / iOS / Android / Web)                                             |
|  - Svelte 5 + TypeScript + Vite + Tailwind CSS                                        |
|  - Virtualized Chat Timeline (dynamic heights, scroll pinning, anchor preservation)   |
|  - Declarative Plugin UI Renderer (JSON-driven slots) + Sandboxed Iframe Widgets     |
+---------------------------------------------------------------------------------------+
|  IPC Bridge (Tauri 2 IPC on Desktop/Mobile | Direct WASM / Worker on Web)             |
+---------------------------------------------------------------------------------------+
|  Rust Application Core (`alloy-core`, `alloy-plugin`, `alloy-theme`)                  |
|  - Matrix Client Logic (`matrix-rust-sdk`: sliding sync, Olm/Megolm E2EE, SQLite)     |
|  - WASM Plugin Host (`wasmtime` with Pulley interpreter on mobile, WASI Component)    |
|  - Design Token Engine & Capabilities / Permission Enforcement                        |
+---------------------------------------------------------------------------------------+
```

### Key Rationale
1. **True Single Codebase across Desktop, Mobile, and Web**: Tauri 2.0 (stable since October 2024) natively supports macOS, Windows, Linux, iOS, and Android. The frontend is standard Web technology, enabling an immediate or future zero-rewrite deployment to browser-based Web (PWA/SPA) by swapping the IPC transport to an in-browser WASM build of `alloy-core` or a lightweight HTTP/WebSocket bridge.
2. **Chat-Critical UI Capabilities Out-of-the-Box**: Modern chat platforms require high-frequency virtualized scrolling with variable-height messages, dynamic image/media expansion, anchor-preserving scroll jumps, bidirectional text (BiDi/RTL), full CJK/accented Input Method Editor (IME) composition, animated stickers, custom emoji fonts, rich text (Markdown/KaTeX/syntax highlighting), and complete assistive accessibility (VoiceOver, TalkBack, NVDA). Re-implementing these atop raw GPU canvas toolkits (Iced, Slint, GPUI, egui) would require years of custom engine development. System WebViews provide these natively.
3. **Declarative Plugin UI & Sandboxing**: Plugin-contributed UI (JSON-defined panels, settings, custom message renderers, slash commands) can be rendered via a recursive declarative component tree. For untrusted visual widgets, WebViews provide standard, bulletproof browser sandboxing (`<iframe sandbox="..." csp="...">`) with filtered `postMessage` messaging, preventing untrusted UI from accessing Matrix cryptographic keys or storage.
4. **Why Svelte 5 over Leptos/Yew/Dioxus-web**: Running a Rust-compiled WASM frontend inside a mobile/desktop WebView incurs duplicate runtime overhead (V8/JavaScriptCore + WASM GC/memory), double FFI translation (`WebView JS -> WASM DOM -> Tauri IPC -> Rust Core`), and cuts off the battle-tested npm ecosystem for virtualized lists (`virtua`, `@tanstack/virtual`) and rich text editors (`TipTap`, `Lexical`). Svelte 5 with fine-grained runes compiles to tiny vanilla JS (<50 KB runtime footprint), provides instant reactivity, and runs with zero garbage collector pauses.
5. **Fallback Rationale (Flutter)**: If runtime WebView profiling on low-end Android devices demonstrates unacceptable frame drops or if system push/background execution strictly conflicts with WebView lifecycle, Flutter is the only alternative offering a single codebase across all five platforms with mature text handling and 120 fps rendering via Impeller. Pure native per-platform UI (SwiftUI for iOS + Jetpack Compose for Android, the Element X approach) is explicitly rejected as primary because it requires maintaining 3 separate UI codebases.

---

## 2. Options Compared

| Evaluation Dimension | (a1) Tauri 2 + Svelte 5 / TS | (a2) Tauri 2 + Leptos / Yew | (b) Dioxus (Wry / Blitz) | (c) Slint | (d) Iced | (e1) Element X Approach (SwiftUI + Compose) | (e2) Flutter + `matrix-rust-sdk` | (f1) GPUI | (f2) egui |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Codebases Required** | **1** (Shared UI + Rust core) | **1** (Shared UI + Rust core) | **1** (Shared UI + Rust core) | **1** (Shared UI + Rust core) | **1** (Shared UI + Rust core) | **3** (SwiftUI, Compose, Web/Desktop) | **1** (Dart UI + Rust core) | **1** (Desktop only today) | **1** (Shared UI + Rust core) |
| **Mobile Maturity (iOS / Android)** | **High** (Tauri 2 stable Wry/Tao mobile; native Swift/Kotlin plugins) | **High** (Runs in Tauri 2 mobile WebView) | **Medium** (Wry mobile functional; Blitz native unusable on mobile) | **Low–Medium** (iOS experimental, Android CMake wrapper, rough touch) | **Low** (Community hacks only; officially out of scope) | **Industry Best** (Element X production flagship; native 120 Hz) | **High** (Impeller engine, battle-tested mobile controls) | **Immature** (Third-party `gpui-mobile` alpha only) | **Low** (Clunky touch, no mobile navigation paradigm) |
| **Web Target Viability** | **Native Web** (Standard SPA/PWA build via Vite) | **Native Web** (Compiles to WASM directly) | **Native Web** (Compiles to WASM directly) | **Medium** (WASM canvas; heavy canvas text, no SEO/DOM) | **Low–Medium** (WASM canvas; no DOM selection) | **Zero** (Requires complete web rewrite in React/TS) | **Medium** (CanvasKit/WASM; heavy ~2 MB init, canvas text) | **Experimental** (WASM canvas in development) | **Low–Medium** (Canvas-based WebGL/WebGPU) |
| **Text Rendering & IME** | **Native OS Engine** (Full CJK IME, font fallback, BiDi, selection) | **Native OS Engine** (Runs in WebView DOM) | **Native in Wry; Broken in Blitz** (Blitz text layout incomplete) | **Basic** (Custom HarfBuzz text shaping; IME rough on mobile) | **Rudimentary** (Basic Cosmic-text; mobile IME unhandled) | **Native OS Engine** (Full platform CoreText / TextLayout) | **High** (Skia/Impeller text shaping, native mobile IME) | **High on desktop** (Cosmic-text / CoreText; code-centric) | **Rudimentary** (Single-font fallback, poor CJK/complex scripts) |
| **Chat Timeline Virtualization** | **Mature** (`virtua`, `@tanstack/virtual`, dynamic row heights) | **Medium** (Manual DOM virtualizer or JS bridge) | **Medium** (Community virtualizers, fragile scroll anchoring) | **Hard** (Limited dynamic item height measurement in `.slint`) | **Hard** (Complex Elm subscription scroll math) | **Native Best** (`LazyVStack`, `LazyColumn` native pinning) | **High** (`ListView.builder`, native scroll physics) | **Custom** (Zed code virtualizer; chat dynamic heights need work) | **Poor** (Immediate mode recalculates on frame; jittery heights) |
| **Runtime Theming & Custom CSS** | **Instant** (CSS custom properties, token swaps, dynamic styles) | **Good** (CSS properties via style bindings) | **Medium** (Inline styles or Tailwind compilation) | **Restricted** (Compiled `.slint` properties; interpreter slow) | **Static** (Rust theme structs; dynamic runtime styling hard) | **Platform-specific** (SwiftUI Environment / Compose Material) | **High** (Flutter `ThemeData`, dynamic runtime override) | **Moderate** (GPUI theme structs; recompile or custom asset) | **Low** (Hardcoded `egui::Visuals`) |
| **Declarative Plugin UI (JSON)** | **Native** (Dynamic Svelte AST components + Sandboxed `iframe`) | **Medium** (Rust JSON parser to dynamic Leptos view) | **Medium** (Rust JSON parser to dynamic RSX) | **Possible** (`slint_interpreter` runtime parser; no sandbox) | **Cumbersome** (`Box<dyn Widget>` interpreter; no sandbox) | **High Effort** (Must write JSON renderers in Swift AND Kotlin) | **Medium** (`json_dynamic_widget` package; Dart sandbox hard) | **Custom** (Custom dynamic layout interpreter in Rust) | **Simple** (Recursive immediate mode loop, but ugly UI) |
| **Hot Reload DX** | **Sub-second** (Vite HMR on desktop and mobile) | **Slow** (Trunk WASM recompile ~3–10s) | **Fast for RSX** (Dioxus CLI HMR; logic requires compile) | **Fast** (Slint live preview tooling) | **None** (Full cargo rebuild/re-link cycle) | **Fast** (Xcode Previews / Compose Hot Reload) | **Sub-second** (Stateful Flutter Hot Reload) | **None** (Full cargo compile) | **None** (Full cargo compile) |
| **Binary Size (App / Install)** | **Small** (~8–15 MB desktop, ~12–18 MB mobile) | **Medium** (~15–25 MB with embedded WASM bundle) | **Medium** (~15–30 MB) | **Small** (~10–18 MB native binary) | **Small** (~12–20 MB) | **Medium** (~25–35 MB per native platform app) | **Medium–Large** (~25–45 MB includes Flutter engine) | **Medium** (~20–30 MB) | **Small** (~10–15 MB) |
| **Security Surface** | **WebView Sandbox + Tauri ACL** (Requires strict CSP/sanitization) | **WebView Sandbox + Tauri ACL** | **WebView or Native** | **Native Memory Safe** (Rust memory model, no DOM) | **Native Memory Safe** (Rust memory model) | **Native Platform Sandbox** (App Sandbox, Keychain) | **Dart VM Sandbox + Native FFI** | **Native Memory Safe** (Rust memory model) | **Native Memory Safe** (Rust memory model) |
| **Licensing** | **MIT / Apache-2.0** | **MIT / Apache-2.0** | **MIT / Apache-2.0** | **GPLv3 / Commercial Dual** [RISK] | **MIT** | **Apache-2.0 / Native** | **BSD-3-Clause** | **Apache-2.0 / GPL** | **MIT / Apache-2.0** |
| **Ecosystem & Hiring** | **Vast** (Millions of TS/Web developers + Rust backend) | **Niche** (Rust web frontend specialists) | **Growing** (Rust enthusiasts) | **Small** (Qt/C++/Rust niche) | **Small** (Rust graphics niche) | **Abundant but Split** (Separate iOS + Android developers) | **Vast** (Massive mobile developer ecosystem) | **Niche** (Zed ecosystem contributors) | **Niche** (Rust tool builders) |

---

## 3. Evidence & Deep-Dive Analysis

### (a) Tauri 2 + Rust Core + Web Frontend
- **Current State (2026)**: Tauri 2.0 stable was officially released on October 2, 2024 ([Tauri 2.0 Announcement](https://v2.tauri.app/blog/tauri-20/)). It introduced first-class mobile target generation for iOS and Android, rewritten plugin system supporting Kotlin/Swift hooks, an ACL-based capability security system, and mobile Hot Module Replacement (HMR).
- **Mobile Target Assessment**:
  - Desktop uses OS-native web engines: WKWebView (macOS), WebView2 (Windows), WebKitGTK (Linux).
  - Mobile uses WKWebView (iOS 14+) and Android System WebView (Chrome/Blink).
  - Native capabilities (push notifications, biometrics, file pickers, secure storage, camera) are bridged via official Tauri plugins ([Tauri Plugins Workspace](https://github.com/tauri-apps/plugins-workspace)).
- **Web Frontend Selection (Svelte 5 vs Leptos vs Dioxus-web)**:
  - *Leptos / Dioxus-web (Rust in WASM)*: Compiling UI code to WASM inside a mobile WebView creates architectural bloat. Memory is allocated inside the WASM linear heap, DOM calls cross the JS-WASM boundary, and communication with `alloy-core` goes: `WASM UI -> JS Glue -> Tauri IPC -> Rust native core`. Furthermore, specialized chat libraries (e.g., dynamic virtualized timeline, markdown streaming, IME workarounds) must be authored from scratch in Rust.
  - *Svelte 5 (TypeScript)*: Svelte 5 introduces fine-grained reactive runes (`$state`, `$derived`, `$effect`), eliminating the virtual DOM and compiling directly to minimal imperative DOM mutations. Bundle size is negligible (<30 KB gzip).
  - Web ecosystem access: Enables direct use of `@tanstack/virtual` or `virtua` ([Virtua GitHub](https://github.com/inokawa/virtua)) for 60/120 fps reverse-scrolling timelines with dynamic cell heights; `TipTap` / `ProseMirror` for markdown/rich text input; and `Twemoji` / native font glyph renderers.
- **Web Target Viability**: The exact same Svelte 5 codebase builds to a static bundle. When deployed as a web client, an adapter layer swaps the Tauri `invoke` bridge to a direct WebAssembly module instantiation of `alloy-core` compiled to `wasm32-unknown-unknown` ([matrix-rust-sdk WASM support](https://github.com/matrix-org/matrix-rust-sdk/blob/main/README.md)) or an HTTP/WebSocket gateway.

### (b) Dioxus (Native Blitz vs Wry)
- **Current State**: Dioxus 0.6+ ([Dioxus Release Notes](https://dioxuslabs.com/blog/release-060)) positions itself as a "Rust Flutter".
- **Analysis**:
  - *Dioxus Desktop / Mobile (WebView mode)*: Uses `wry` and `tao`. If running via WebView, Dioxus provides no architectural advantage over Tauri 2, while suffering from a smaller plugin ecosystem, slower compilation cycles, and limited mobile lifecycle tooling.
  - *Dioxus Native (Blitz / WGPU)*: Experimental native renderer based on Blitz/Vello ([Dioxus-Native Crate](https://lib.rs/crates/dioxus-native)). It parses HTML/CSS to layout nodes using Taffy and draws via WGPU. In 2026, Blitz remains experimental: text editing lacks native mobile IME composition, accessibility bridges (VoiceOver/TalkBack) are missing, font fallback fails for mixed scripts, and rich media (video/audio streams) lacks embedded hardware decoders.
- **Verdict**: Unsuitable for production chat across mobile/desktop in 2026.

### (c) Slint
- **Current State**: Slint 1.x ([Slint Official Site](https://slint.dev/)) provides a declarative UI description language (`.slint`) targeting embedded, desktop, and mobile.
- **Analysis**:
  - *Mobile Maturity*: Android support requires a CMake/Gradle wrapper ([Slint Android Guide](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/android)); iOS support is marked "Work in Progress" / experimental ([Slint iOS Docs](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios)). Mobile keyboard interaction, safe area insets, and touch gesture deceleration require manual platform engineering.
  - *Chat Timeline*: Slint layout models excel at fixed or constrained responsive dashboards. Dynamic virtual lists containing arbitrary heterogeneous content (text, image grids, code blocks, reply quotes) with anchor-preserving scroll jumps are difficult to implement.
  - *Licensing Blocker*: Slint is dual-licensed under GNU GPLv3 or Commercial License ([Slint GitHub Repository](https://github.com/slint-ui/slint)). For an open-source project desiring a vibrant third-party plugin ecosystem, GPLv3 introduces viral copyleft constraints that discourage commercial plugin developers and restrict proprietary distribution channels.
- **Verdict**: Disqualified due to licensing risk, experimental mobile status, and chat timeline limitations.

### (d) Iced
- **Current State**: Iced 0.13.x ([Iced Docs.rs](https://docs.rs/crate/iced/0.13.1/source/README.md)) is an Elm-inspired pure Rust GUI library.
- **Analysis**:
  - *Mobile Support*: Mobile is explicitly documented as a non-priority by the maintainers ([Iced Issue #302](https://github.com/iced-rs/iced/issues/302)). There is no official iOS or Android runner.
  - *Web Target*: Renders to an HTML `<canvas>` element via WebGL/WGPU. It does not output DOM nodes. Consequently, browser-native accessibility (screen readers), native copy-paste, and mobile soft-keyboard interaction are fundamentally degraded.
  - *Runtime Customization*: The Elm architecture requires strictly typed message flows; injecting declarative runtime JSON widgets into an existing Iced `view` requires building a custom dynamic AST interpreter.
- **Verdict**: Disqualified due to lack of mobile support and non-DOM web rendering.

### (e) Native Mobile / Element X Approach vs Flutter
- **Element X Architecture**: Element X ([Element X iOS](https://github.com/element-hq/element-x-ios) and [Element X Android](https://github.com/element-hq/element-x-android)) separates concerns cleanly:
  - `matrix-rust-sdk`: Core protocol, crypto, sliding sync, and SQLite cache written in Rust.
  - FFI Layer: `matrix-sdk-ffi` using Mozilla UniFFI to generate Swift and Kotlin bindings ([Matrix Rust SDK FFI](https://github.com/matrix-org/matrix-rust-sdk/blob/main/README.md)).
  - UI Layer: 100% native SwiftUI for iOS and Jetpack Compose for Android.
  - *Evaluation*: Element X achieves unmatched 120 Hz native responsiveness and perfect platform integration. However, **it requires maintaining two independent mobile codebases plus a third for desktop/web**. For Alloy, where the explicit mandate is **ONE codebase** covering desktop, mobile, and later web, the pure native multi-repo approach is disqualified as the primary architecture.
- **Flutter + `matrix-rust-sdk` (The Viable Fallback)**:
  - Flutter compiles to native ARM64 for iOS/Android and native binaries on macOS, Windows, and Linux ([Flutter Desktop Guide](https://docs.flutter.dev/platform-integration/desktop)).
  - Dart integrates with Rust cleanly via `flutter_rust_bridge`.
  - Impeller GPU engine delivers guaranteed 60/120 fps animations and smooth list virtualization (`ListView.builder`).
  - *Weaknesses*: Dart runtime adds ~20 MB binary size; Web target produces a heavy canvas-based experience (~2 MB initial download, non-standard text selection); runtime plugin UI requires custom dynamic interpreters (e.g., `json_dynamic_widget`).
- **Verdict**: Element X pattern is rejected due to 3-codebase cost. Flutter is designated as the fallback option.

### (f) egui & GPUI
- **egui**:
  - Immediate-mode rendering re-draws the entire UI each frame. While fast for desktop tools, it drains mobile battery rapidly unless aggressive frame throttling is implemented.
  - Mobile touch inertia, text selection handles, and accessibility trees are missing or primitive ([egui Repository](https://github.com/emilk/egui)).
- **GPUI (Zed)**:
  - High-performance retained GPU framework powering Zed ([GPUI](https://www.gpui.rs/)).
  - Desktop (macOS/Linux/Windows) is world-class, but mobile support is non-existent upstream; only early community forks exist (`gpui-mobile`, [GPUI Mobile Docs.rs](https://docs.rs/crate/gpui-mobile/latest)).
  - GPUI is engineered for code editing buffers (uniform line heights, monospaced text runs). Adapting it to heterogeneous chat timelines with variable-size media, inline videos, and dynamic user widgets would require re-architecting its layout pipeline.
- **Verdict**: Disqualified due to absence of official mobile/web targets and text/media layout constraints.

---

## 4. Deep Dive: Modern Chat UI Requirements

A competitive chat UI (Discord / Telegram / Element X feel) places severe constraints on the rendering stack:

```
+---------------------------------------------------------------------------------------+
| CHAT UI CAPABILITY MATRIX                                                             |
+------------------------------------+------------------------+-------------------------+
| Feature Requirement                | Tauri 2 + Svelte 5     | Pure Rust GUI (Iced/GPUI)|
+------------------------------------+------------------------+-------------------------+
| Virtualized dynamic message list   | Solved (Virtua / DOM)  | Manual height caching   |
| Scroll pinning & anchor jump       | Native scroll physics  | Manual wheel/touch math |
| Full CJK / Accented IME            | Native OS Web engine   | Rudimentary / broken    |
| BiDi / RTL layout (Arabic/Hebrew)  | Full Unicode CSS engine| Complex custom shaping  |
| Accessibility (VoiceOver/TalkBack) | Full W3C ARIA tree     | Custom AccessKit (rough)|
| Inline Media (Video, WebRTC audio) | Native `<video>`/WebRTC| Custom GStreamer/FFmpeg |
| Runtime CSS / Token Theming        | Instant (CSS vars)     | Full re-layout pass     |
| Custom Emoji & Animated Stickers   | Lottie / APNG / SVG    | Custom raster pipeline  |
+------------------------------------+------------------------+-------------------------+
```

1. **Virtualized Timeline with Variable Height**:
   - Chat messages range from 24 px (single emoji) to 800+ px (code block, image collage, reply quote, media preview).
   - When images load asynchronously, they shift heights. In standard Web engines, `ResizeObserver` detects layout shifts and dynamic virtualizers (`virtua` or `@tanstack/virtual`) adjust offsets without scroll hitching.
   - Downward scrolling must anchor to the newest incoming message; upward scrolling must maintain the scroll offset relative to the current visible anchor message during pagination. Browser scroll anchor APIs (`overflow-anchor: auto`) handle this automatically.
2. **Text Rendering, BiDi, and IME**:
   - Chat involves multinational character sets, emojis, inline code, links, and mentions.
   - Mobile keyboards require active IME composition sessions (e.g., Japanese Pinyin, Korean Hangul, predictive text suggestions). Native WebViews inherit 20+ years of platform IME integration. Pure Rust canvas renderers require raw platform winit/NDK/UIKit event integration, which is notoriously fragile across Android OS revisions.
3. **Accessibility (a11y)**:
   - Modern accessibility demands screen reader navigation (VoiceOver on iOS/macOS, TalkBack on Android, NVDA/JAWS on Windows), focus management, high-contrast modes, and text scaling.
   - Svelte generates semantic HTML with ARIA roles (`role="log"`, `aria-live="polite"`). Rust native toolkits rely on `accesskit`, which is functional on desktop but still developing for iOS and Android [INFERENCE].

---

## 5. Declarative Plugin UI Architecture

Alloy's plugin system requires two distinct tiers of UI contribution:
1. **Declarative Component Rendering (JSON/JSONC Schema)**: Safe, lightweight UI elements (settings pages, custom sidebar panels, room header buttons, slash-command results).
2. **Isolated Web Components / Iframes**: Interactive, rich third-party visual widgets (custom dashboards, interactive bots, mini-apps).

### Tier 1: Declarative Schema-Driven UI (Native Svelte Engine)
- The plugin manifest exports UI declarations adhering to a strict schema (`alloy-ui-schema.json`).
- Example Panel Declaration:
```json
{
  "$schema": "https://alloy.chat/schemas/v1/ui.json",
  "id": "poll-widget",
  "type": "panel",
  "title": "Room Poll",
  "body": {
    "type": "vstack",
    "spacing": "md",
    "children": [
      { "type": "text", "content": "${state.question}", "style": "heading" },
      {
        "type": "list",
        "items": "${state.options}",
        "itemTemplate": {
          "type": "button",
          "label": "${item.text} (${item.votes})",
          "action": { "call": "cast_vote", "args": { "option_id": "${item.id}" } }
        }
      }
    ]
  }
}
```
- **Rendering Mechanism**:
  - The Svelte UI maintains a dynamic registry of trusted primitive components: `VStack`, `HStack`, `Text`, `Button`, `Input`, `Toggle`, `Badge`, `Card`.
  - A recursive `<DynamicNode schema={body} context={pluginState} />` component renders the tree.
  - User actions emit events to the Tauri IPC layer, which forwards them directly to the plugin's WASM runtime running in `alloy-plugin`.
  - **No arbitrary executable JS is loaded in the main window.** The declarative tree is pure data.

### Tier 2: Sandboxed Visual Widgets (Iframes)
- For full-blown visual extensions (e.g., Matrix MSC1236 / MSC2876 widgets, embedded games, drawboards):
  - The shell instantiates a sandboxed iframe:
    ```html
    <iframe
      sandbox="allow-scripts allow-forms"
      csp="default-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'none';"
      src="asset://plugin-{id}/index.html"
    ></iframe>
    ```
  - Communication crosses a strict bidirectional `MessageChannel` bridge.
  - The bridge exposes a capability-checked RPC API (e.g., `send_message`, `get_room_members`), enforced by `alloy-core`.
  - **Memory & Key Isolation**: Even if the iframe code contains a zero-day exploit, it has zero access to `window.__TAURI__`, local storage, SQLite databases, or Olm/Megolm encryption keys.

---

## 6. Security Surface: WebView vs Native

| Threat Vector | Tauri 2 + Svelte 5 | Pure Native GUI (Slint / Iced / GPUI) |
| :--- | :--- | :--- |
| **XSS / HTML Injection** | **Medium risk** (Attacker injects malicious HTML in chat message). Mitigated by strict DOMPurify, disabling `innerHTML`, and CSP. | **Zero risk** (Rendered as raw glyphs/shapes). |
| **Sandbox Isolation** | **High isolation**: Tauri 2 enforces granular capability permissions per window and command. WebViews run in dedicated OS renderer processes. | **Low isolation**: Plugin code running in the same process can dereference memory or panic the client. |
| **Memory Safety Vulnerabilities** | **Very Low**: Rust core is memory-safe; WebViews are maintained and patched directly by Apple (WebKit) and Google (Blink). | **Low**: Pure Rust code is memory-safe, but unsafe C bindings in graphic drivers/shaders carry crash risks. |
| **Zero-Day Exploit Patching** | **Automatic**: OS updates update the system WebView without requiring Alloy app updates. | **Manual**: Security bugs in custom parsers or renderers require shipping a new app binary release. |
| **Plugin Isolation** | **Standardized**: Browser `iframe` sandbox and CSP provide world-class isolation for visual widgets. | **Complex**: Requires custom WASM-to-GPU memory virtualization to isolate drawing commands. |

### Security Mitigation Strategy for Tauri 2
1. **Strict Content Security Policy (CSP)**: Tauri's configuration enforces `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' mxc: blob: data:; media-src 'self' mxc: blob:;`.
2. **Tauri 2 Scopes & Capabilities**: All filesystem, network, and clipboard access must be explicitly authorized per plugin in `src-tauri/capabilities/default.json`.
3. **Matrix Event Sanitization**: All incoming Matrix message bodies (HTML formatted strings) are sanitized in Rust using `ammonia` before reaching the frontend, with secondary client-side sanitization via `DOMPurify`.

---

## 7. Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
| :--- | :---: | :---: | :--- |
| **WebView Inconsistencies on Android** | High | Medium | Android devices use varying system WebView versions. Enforce minimum Android System WebView version (API 26+ / Chrome 90+) via Android manifest checks. |
| **Mobile Memory Pressure & Backgrounding** | High | Medium | iOS kills background WebViews aggressively. State must be persisted to SQLite via `alloy-core` immediately; WebView acts as an ephemeral display that can cold-restore from cached local state in <100 ms. |
| **Mobile Push Notifications & Background Sync** | High | High | WebViews cannot handle background push wakes. Use native Tauri mobile plugins (Swift `UNUserNotificationCenter` and Kotlin Firebase Messaging) to wake `alloy-core` directly in background services without spinning up the WebView. |
| **High Scroll Hitching on Low-End Devices** | Medium | Low | Use `virtua` with CSS `contain-intrinsic-size` and `content-visibility: auto`. Offload Markdown parsing and image decryption to Rust background threads; send pre-computed layout metrics to the UI. |
| **Tauri 2 IPC Serialization Overhead** | Medium | Low | Large chat events (e.g. initial sync) can saturate JSON IPC. Use binary IPC transfers (Tauri 2 IPC supports raw typed arrays / byte buffers) for timeline sync arrays and media buffers. |

---

## 8. Concrete Decisions

1. **Adopt Tauri 2 as the Primary Shell Framework**:
   - Monorepo structure: Rust workspace for backend (`crates/alloy-core`, `crates/alloy-plugin`, `crates/alloy-theme`, `apps/desktop-mobile-tauri/src-tauri`) and frontend workspace (`apps/desktop-mobile-tauri/ui`).
2. **Select Svelte 5 + TypeScript + Vite as the UI Framework**:
   - Svelte 5's fine-grained reactivity and minimal runtime overhead provide maximum frame budget for 120 Hz mobile screens.
   - Vite enables sub-second HMR across both desktop windows and physical mobile test devices.
3. **Decouple UI Shell from `alloy-core` via Protocol Boundary**:
   - The UI communicates strictly over an async Command/Event interface (typed with TypeScript definitions generated from Rust types using `specta` / `ts-rs`).
   - On Desktop/Mobile: Transport is Tauri IPC.
   - On Web (future phase): Transport is direct WASM bridge or WebSocket.
4. **Declarative First for Customization**:
   - Plugins declare UI via JSON schemas parsed by Svelte components.
   - WASM plugins run in `alloy-plugin` on the host side, pushing state changes to the UI via the standard event bus.
5. **Establish Flutter as the Explicit Fallback**:
   - If performance benchmarks on baseline target devices (iPhone 11, low-end Android Go) fail 60 fps scroll targets during prototype validation, pivot the UI shell to Flutter, preserving `alloy-core` via `flutter_rust_bridge`.

---

## 9. Open Questions for the User

1. **Baseline Mobile Device Spec**: What is the oldest targeted hardware? (e.g., iPhone 11 / iOS 17+ and Android 10+ vs older Android 8 devices where WebView engines may be dated)?
2. **Web Target Priority**: Does "web later in the lifecycle" mean a hosted public web app (e.g., `app.alloy.chat`), or is self-hosted / decentralized web deployment an immediate design constraint?
3. **Desktop Tray & Multi-Window UX**: Does Alloy require heavy multi-window support (e.g., popping out individual chat rooms or voice call windows into separate OS windows)? Tauri 2 supports multi-window on desktop, but mobile is strictly single-window.

---

## 10. Work Breakdown: UI Shell Bootstrapping

| Task ID | Phase / Description | Size | Parallelizable | Dependencies |
| :--- | :--- | :---: | :---: | :--- |
| **UI-01** | **Repository Structure & Tooling Setup**: Initialize Cargo workspace with Tauri 2 CLI, configure `src-tauri` with mobile targets (iOS/Android) and Vite + Svelte 5 frontend template. | **S** | No | None |
| **UI-02** | **IPC Interface & Type Generation**: Configure `ts-rs` or `specta` to export Rust domain types (`RoomSummary`, `TimelineItem`, `Event`) to TypeScript definitions. | **S** | Yes | UI-01 |
| **UI-03** | **Design Token & Theme System Integration**: Implement CSS custom property engine mapped to `alloy-theme` tokens; support dark/light/high-contrast and user theme overrides. | **M** | Yes | UI-01 |
| **UI-04** | **Virtualized Timeline Prototype**: Implement `virtua` dynamic timeline with simulated heterogeneous chat messages (text, code, images) demonstrating 60/120 fps scroll and bottom-pinning. | **M** | Yes | UI-02, UI-03 |
| **UI-05** | **Mobile Shell & Gesture Adaptation**: Configure iOS / Android safe-area insets, keyboard view avoidance, pull-to-refresh, and swipe-to-reply touch interactions in Svelte. | **M** | No | UI-04 |
| **UI-06** | **Declarative Plugin UI Renderer**: Build the `<DynamicNode />` recursive Svelte component interpreting `alloy-ui-schema.json` with event callbacks to the IPC layer. | **M** | Yes | UI-03 |
| **UI-07** | **Sandboxed Iframe Widget Host**: Implement isolated iframe container with CSP enforcement and bidirectional `MessageChannel` for untrusted plugin visual widgets. | **M** | Yes | UI-06 |
| **UI-08** | **Native Mobile Notification & Background Hook**: Implement native Tauri mobile plugins for iOS APNs and Android FCM to wake `alloy-core` background sync. | **L** | No | UI-05 |
| **UI-09** | **Web Target Prototype Build**: Configure Vite build target for standalone browser deployment with mock WASM IPC transport. | **S** | Yes | UI-02, UI-04 |
