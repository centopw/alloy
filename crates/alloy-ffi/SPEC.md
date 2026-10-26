---
id: alloy-ffi
type: module-design
status: draft
title: alloy-ffi — Android JNI bridge
parent: alloy-architecture
tags: [android, jni, ffi]
depends-on: [alloy-push]
---

## Responsibility

The Android JNI surface for headless background push. It wraps `alloy-push` behind `Java_chat_alloy_PushNative_*` exports, runs a lazy multi-thread tokio runtime, catches panics at the boundary and returns errors as JSON strings so a panic never crosses into the JVM.

## Boundary

It depends on `alloy-push` and uses the C ABI/JNI; no Tauri and no UI. It is the only crate in the workspace permitted `unsafe` code, and the exception is documented in its crate root. Exported functions stay thin: wrap the Rust implementation, catch panics, return a string. The crate is Android-only and is loaded from the app library.
