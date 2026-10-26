---
id: alloy-goal
type: goal-and-requirements
status: draft
title: Alloy — goal and requirements
tags: [matrix, chat, product]
---

## Goal

Alloy is a cross-platform Matrix chat client for desktop (macOS, Windows, Linux) and Android. It aims to make everyday encrypted conversation feel as fast and calm as a mainstream messenger while staying fully open source and private: no analytics, no tracking, no ads, and no data collection.

## Problem

Matrix is an open, end-to-end-encrypted network, but its clients are a compromise for everyday users: either feature-rich and rough, or polished and closed. People who want a private, standards-based chat app need one that is quick to read and answer in, works on the devices they already use, and lets them shape the interface without giving up privacy. The audience is technical communities and power communicators as much as privacy-conscious Matrix users.

## Audience

- People with an existing Matrix account (matrix.org, a company server, a self-hosted server) who want a faster, calmer client.
- Distributed technical teams and communities that live in group rooms and direct messages.
- Privacy-conscious users who refuse telemetry and proprietary push services.

## Capabilities

Alloy already does the following; these are capabilities of the shipped code, not a roadmap.

- **Accounts and sessions:** sign in to any Matrix homeserver with password (OIDC is tracked but not shipped); restore sessions, list and remove devices, edit the profile, log out. Secrets live in the OS keyring on desktop and in an owner-only file where no keyring backend exists (Linux without Secret Service, Android).
- **Encrypted conversation:** end-to-end encrypted direct and group rooms; send and receive text with markdown; reply, edit, redact, react, pin, forward; threads; polls; voice messages; photo albums, files and location sharing; read receipts and typing indicators.
- **Finding things:** search the room list from the sidebar or the command palette; search inside a room by words or by sender and jump to the message. Server-side search is unavailable in encrypted rooms.
- **Organisation:** user-defined room tags with colours, filtering and sorting of the room list by tag, unread, name or activity; pin, archive, mark unread, mute for an hour or a day; unsent text kept as a draft.
- **Notifications:** desktop notifications and Android background notifications through UnifiedPush; per-room notification levels; unread counts that clear when a room is read; notification clicks open the room; an update notice that shows what changed.
- **Appearance and settings:** light, dark and AMOLED themes plus one extra theme; three message layouts (modern, chat bubbles, compact); text size and density; a command palette; settings pages whose controls change real behaviour.
- **Mobile shell:** below 900 px wide the app becomes a phone interface — one screen at a time, a bottom tab bar (Chats, Discover, Settings), full-screen pages, action sheets and 44 px touch targets.
- **Delivery:** signed desktop installers and an in-app updater feed; an Android APK distributed through GitHub Releases with an update check.

## Non-goals

Only what a decision excludes. Anything neither listed nor excluded is open, not forbidden.

- **No telemetry of any kind** — no analytics, tracking, ads or data collection (decision D8).
- **No proprietary Google services as the default path** — Android push uses UnifiedPush first; FCM is optional and deferred (decision D10, ADR-0009).
- **No iOS target in the current scope**, and no App Store/TestFlight dependency for releases (decision D10).
- **No MatrixRTC voice/video calls in the current scope** (decision D6).
- **No web target in the current scope** — the web build is a later phase (PLAN §5, P5).
- **No app-store distribution requirement** — releases ship as signed installers and a GitHub-Releases APK (decision D10).

## Durable principles

- **Privacy and user control come first.** End-to-end encryption, zero telemetry, no ads, Apache-2.0, and user-owned customization. The Android path avoids Google dependencies wherever the open ecosystem provides an alternative.
- **All product logic lives in Rust; the UI renders it.** A typed command/event protocol (`alloy-protocol`) separates the Rust engine from the Tauri 2 shell and Svelte 5 view layer. The protocol is frozen per phase and written by one owner before dependent work starts (decisions D1, D4).
- **Safety is enforced by the toolchain.** `unsafe` is forbidden workspace-wide; `crates/alloy-ffi` is the sole documented exception, because Android JNI exports require the C ABI.
- **Design follows the recorded visual language.** Text-first surfaces, functional icons only (no emoji decoration), a Discord-style multi-column desktop shell, a separate phone shell, settings that change behaviour or do not exist, one implementation per UI concept, and design tokens as the source of colour, type and spacing (ADR 0001, 0002, 0005, 0006, 0007; decisions D11–D16).
- **Work is traceable.** Every change is defined by a GitHub issue and every lasting decision is recorded in an ADR; the orchestrator, not individual agents, writes the board.
