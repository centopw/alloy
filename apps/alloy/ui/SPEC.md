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

It talks only to the protocol through `ipc/`; it has no Rust crate dependencies, makes no homeserver calls and holds no product logic. Platform differences live in `platform/`, native controls are wrapped in shared components, and design tokens arrive as CSS variables from `alloy-theme`. It must not call matrix-sdk or bypass the engine. Pull-to-refresh and content-URI picking are wired through `platform/files.ts` and the shell's `pullToRefresh` action; the composer's tiered attach menu calls `pickMediaFiles` for pictures and videos and `pickFiles` for a plain file, with polls and locations under its More flyout.

Known limitations: the accessibility audit is pending (#20, #36); user-supplied keybinding and slash-command JSON packages are not loaded yet.
