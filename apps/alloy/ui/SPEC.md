---
id: alloy-ui
type: submodule-design
status: draft
title: alloy-ui — Svelte 5 view layer
parent: alloy-app
tags: [svelte, ui, view]
depends-on: [alloy-protocol]
---

## Responsibility

The Svelte 5 / TypeScript / Vite view layer. It consumes generated protocol types over a `Transport` (Tauri IPC in the app, `MockTransport` in development, the WS bridge for browser E2E), owns the runes stores that apply `VectorDiff` streams, and renders the shell, timeline, composer, settings, onboarding, tags and update surfaces. It also owns the theme store that consumes generated CSS variables, the platform-capability layer, and the audit tests (controls, native menus, tooltips, emoji guard). Below 900 px it renders the phone shell.

## Boundary

It talks only to the protocol through `ipc/`; it has no Rust crate dependencies, makes no homeserver calls and holds no product logic. Platform differences live in `platform/`, native controls are wrapped in shared components, and design tokens arrive as CSS variables from `alloy-theme`. It must not call matrix-sdk or bypass the engine. Pull-to-refresh and content-URI picking are wired through `platform/files.ts` and the shell's `pullToRefresh` action; the composer's tiered attach menu calls `pickMediaFiles` for pictures and videos and `pickFiles` for a plain file, with polls and locations under its More flyout. Staged pictures and videos each carry a thumbnail in the preview strip — a picture's image or a video's poster frame — and fall back to a file-type glyph when no preview can be drawn.

### UI stack

Interaction-heavy primitives are vendored from shadcn-svelte (Bits UI behaviour) into `src/lib/components/ui/**`; business surfaces — shell, timeline, composer, upload strip, media view, settings — stay hand-written. `crates/alloy-theme` remains the single source of design truth: `src/app.css` aliases shadcn's CSS variable contract onto the generated `--alloy-*` tokens, imports Tailwind's theme and utilities layers without Preflight (so `tokens.css` stays the authoritative reset), and keys dark mode off the `data-theme` attribute rather than a `.dark` class. Vendored files are never edited to satisfy a repo audit or to restyle; overrides go through the component's `class` prop. The deliberately excluded primitives (`ContextMenu`, `Select`, `Modal`, `TooltipLayer`, `Avatar`, `EmojiPicker`) encode product behaviour — the app-wide menu model, backstack integration and drag-to-dismiss sheets, global tooltip derivation, deterministic sender colours — that a generic primitive does not model. See decision 0012.

Known limitations: the accessibility audit is pending (#20, #36); user-supplied keybinding and slash-command JSON packages are not loaded yet; only `switch` and `progress` are vendored so far, so the migration is incremental and most primitives remain hand-written.
