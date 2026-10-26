---
id: alloy-bridge
type: module-design
status: draft
title: alloy-bridge — loopback development bridge
parent: alloy-architecture
tags: [dev, websocket, testing]
depends-on: [alloy-protocol, alloy-storage, alloy-engine]
---

## Responsibility

A development and testing bridge that exposes the engine over a loopback WebSocket to the browser UI without Tauri. It reuses the same `EngineCommand`/`EngineResponse`/`EngineEvent` JSON as the Tauri IPC commands, binds to `127.0.0.1` only, requires the bridge token in the query string and enforces a local-UI origin allowlist. It is a tool, not a server.

## Boundary

It depends on engine, protocol and storage. It is a dev-only binary with no product logic and no deployment path; the web target's production transport is a later decision. Security stays local: loopback binding, token authentication and origin checking. It must not be reachable from another host.
