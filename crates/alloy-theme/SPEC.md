---
id: alloy-theme
type: module-design
status: draft
title: alloy-theme — design tokens and CSS variables
parent: alloy-architecture
tags: [design, tokens]
---

## Responsibility

The design-token system. It owns the W3C-flavoured JSON/JSONC theme schema, the loader with `extends` deep merge and unknown-key rejection, colour validation, WCAG 2.1 AA/AAA contrast reporting, and CSS-variable generation (`--alloy-*`). It ships the built-in light, dark, AMOLED and one extra theme plus the `gen-css` binary. The published schema lives in `schemas/theme.schema.json`; the engine's settings handler loads workspace themes from `<data_dir>/themes` and development directories through this crate.

## Boundary

It is standalone: serde, serde_json, jsonc-parser and thiserror only. It must not depend on protocol, core or engine, and it contains no UI components. It is the single source of colour, type and spacing; the UI consumes the generated CSS variables. Theme ids are validated by the engine when workspace files are loaded.

Known limitations: keybinding and slash-command JSON packages are not wired; only themes load from user files. Theme reload follows an engine command rather than a watched file change.
