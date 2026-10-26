---
id: alloy-plugin
type: module-design
status: draft
title: alloy-plugin — sandboxed WASM plugin host
parent: alloy-architecture
tags: [plugins, wasm, sandbox]
---

## Responsibility

The sandboxed WASM Component Model host. It owns manifest parsing and validation against `schemas/plugin-manifest.schema.json`, capability declarations (names, scopes, hosts, quotas and slots), the Wasmtime engine with fuel metering and epoch interruption, the 32 MiB memory cap, the Pulley interpreter fallback for no-JIT targets, and the WIT world in `wit/alloy-plugin.wit`. Plugin contributions are declarative — commands, panels, message renderers and themes — and never DOM or raw handles.

## Boundary

It is standalone: the Wasmtime/WIT stack plus serde and jsonc. It has no dependency on protocol, core or engine. The engine does not load it yet, so there is no inbound edge; when wired, capabilities mediate access. It must not expose network or filesystem access without a declared capability, and plugins never receive homeserver credentials.

Known limitations: the host is not wired into the engine; there is no marketplace, signed index or revocation path yet.
