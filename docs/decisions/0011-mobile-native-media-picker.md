---
id: adr-0011-mobile-native-media-picker
type: architecture-design
status: draft
title: "ADR 0011: Pictures and videos use the system media chooser on mobile"
parent: alloy-architecture
depends-on: [adr-0005-mobile-first-shell, adr-0010-secure-mobile-webview-origin]
tags: [adr, android, picker, media]
---

# 0011: Pictures and videos use the system media chooser on mobile

- Status: accepted
- Date: 2026-10-26
- Source: maintainer review: tapping attach on Android opened the generic document picker instead of
  the phone's media picker, and media should be the primary thing the composer sends.

## Context

`pick.rs` opened the dialog plugin's document picker on every platform, so attaching a picture on
Android landed the user in the Files app instead of the gallery. `tauri-plugin-dialog` 2.8.3 already
exposes `PickerMode::{Document, Media, Image, Video}`; its Android plugin maps `media` to an
`ACTION_GET_CONTENT` chooser carrying `EXTRA_MIME_TYPES = ["image/*", "video/*"]`, which surfaces the
system photo/gallery experience and returns `content://` URIs.

## Decision

Add `pick_media_files` to `apps/alloy/src-tauri/src/pick.rs`: the same flow as the existing pickers,
with `set_picker_mode(PickerMode::Media)` and a desktop media-extension filter. It reuses
`resolve_picked` → `copy_into_pick_dir`, which already copies Android `content://` URIs into
`<cache>/picks/` through `tauri-plugin-fs` and allowlists the copy for the engine.

The composer's attach menu leads with **Photo or video** (`pickMediaFiles`); **File** keeps the
document picker (`pickFiles`). The choice of picker is therefore a property of the *intent*, not of
the platform.

## Alternatives considered

- **A custom Kotlin `MediaPickerBridge` using `PickVisualMedia`**: gives the modern system photo grid,
  but means committed Kotlin, activity-result plumbing and a JS event channel for a result the dialog
  plugin already returns. Rejected as the first step; revisit only if the gallery chooser proves
  insufficient (noted in the task-spec as a deferral).
- **An HTML `<input type="file" accept="image/*,video/*">`**: returns blobs to JS, so a large video
  would have to cross IPC as base64 rather than being streamed to a path. Rejected.

## Consequences

- The picker keeps returning local paths to the view layer: desktop paths as picked, mobile `content://`
  URIs copied into the app cache. No view-layer change was needed beyond calling the new command.
- `PickerMode` is mobile-only; desktop keeps the normal dialog and narrows it with extensions, so the
  desktop can never offer a media file outside those extensions through this action.
- The engine's attachment allowlist is unchanged: copies under `<cache>/picks/` are registered exactly
  as before.
