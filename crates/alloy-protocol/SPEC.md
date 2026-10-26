---
id: alloy-protocol
type: module-design
status: draft
title: alloy-protocol — typed protocol v1
parent: alloy-architecture
tags: [contract, ipc, types]
---

## Responsibility

The single source of truth for Protocol v1: the typed boundary between the Rust engine and every UI shell. It owns the DTOs and tagged enums for commands, responses, events and errors, plus the `VectorDiffDto` list-diff streams. TypeScript types are generated from it into `apps/alloy/ui/src/lib/generated/`. The detailed command table lives in `docs/plan/06-ipc-contract.md`, which is an input document rather than an adopted spec.

## Boundary

It is the base of the workspace DAG and has no internal crate dependencies — only serialisation and type-generation libraries. Every other Rust crate may depend on it; it must never depend on `alloy-core`, `alloy-storage` or `alloy-engine`. A change to a command, response or event is a recorded decision that regenerates the TS types; ad-hoc protocol drift is forbidden. The crate performs no I/O and holds no runtime state.
