---
id: alloy-app
type: module-design
status: draft
title: alloy-app — Tauri application shell
parent: alloy-architecture
tags: [tauri, shell, desktop, android]
depends-on: [alloy-protocol, alloy-core, alloy-storage, alloy-engine]
---

## Responsibility

The Tauri 2 application shell for desktop (macOS, Windows, Linux) and Android. It owns the IPC commands (`alloy_invoke`, `alloy_subscribe`), secret-store selection (desktop keyring probed at startup; Linux without Secret Service falls back to the owner-only file with a warning; Android uses the file store), and the platform commands for notifications, attachment picking, save-as and in-app updates. It builds staticlib/cdylib/rlib for mobile, links `alloy-ffi` on Android, and enables the updater and autostart plugins on desktop. The picker commands come in two flavours: `pick_attachment(s)` for a plain file and `pick_media_files` for pictures and videos, which selects the system media chooser on Android and iOS and a media-filtered document dialog elsewhere (ADR 0011). Android content-URI picks are copied into the app cache here. The webview is served over the `https` scheme on Android and Windows so credential forms are a secure context (ADR 0010).

## Boundary

It depends on protocol, engine, core and storage. Tauri-specific code lives only here; product logic belongs in core and engine. It may not bypass the engine for product state. Android background push goes through `alloy-ffi`; desktop updates use the signed Tauri updater feed. The Svelte view layer is a separate package (`apps/alloy/ui`), not part of this crate's Rust surface.

Known limitations: releases are pre-release until signing certificates exist, and the update feed depends on signed manifests.
