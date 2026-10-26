---
id: adr-0001-ui-visual-language
type: architecture-design
status: draft
title: "ADR 0001: UI visual language v1 (text-first, Telegram-inspired chat)"
parent: alloy-architecture
tags: [adr, ui, design]
---

# 0001: UI visual language v1 (text-first, Telegram-inspired chat)

- Status: accepted (items 2, 4, 5 and 6 superseded by 0002)
- Issue: tracked on the project board (type:decision "UI visual language v1")
- Date: 2026-10-16
- Source: maintainer review of the P1a prototype (all themes) against Telegram Desktop as the reference for a calm, modern chat feel.

## Context
The P1a prototype used emoji/decorative icons on rails, rows and buttons, a two-button theme switch, and a flat wall of identical white sender names. It reads as noisy and unfinished.

## Decision
1. **Icons are functional, never decorative.**
   - No emoji used as icons anywhere in chrome (rails, buttons, list rows, empty states).
   - One icon set (Lucide, inline SVG, 1.5px stroke, `currentColor`, size tokens 16/20/24). One import point `src/lib/icons/`.
   - An icon is allowed only when it is the clearest control for a universal action (search, attach, send/mic, settings, close, more, reply, edit, delete, react, verified/encrypted state, send status ticks). Controls with a clear text label stay text-only. No icons on section headers, list rows, or room rows (avatars/initials carry identity). Max one icon per control; icon-only controls require `aria-label` + tooltip.
2. **Theme switching is a toggle, not buttons.**
   - Primary: a single light/dark toggle (switch/segmented thumb animation) in the user menu and settings; "AMOLED" is a sub-option of dark ("Pure black") in Settings > Appearance; "Follow system" is the default.
   - Density and message style (bubble/gutter) live in Settings > Appearance as segmented controls with text labels. The dev HUD controls are dev-only.
3. **Sender identity is colored.** Each user gets a deterministic color from a palette of 12 tokens (`--alloy-color-user-1..12`) derived by hashing the Matrix user id; used for the display name and the avatar background (initials). Palette entries must meet WCAG AA (4.5:1) on the surface they sit on, per theme. Own messages show no sender name.
4. **Telegram-style conversation surface.**
   - Bubble layout is default (gutter remains a user option). Consecutive messages by one sender are grouped: avatar + name only on the first (and avatar on the last in the group for bubble style, as in Telegram), small radius on joined corners.
   - Own messages right-aligned in an accent-tinted bubble; others left-aligned on a surface bubble. Time and delivery ticks sit inside the bubble bottom-right. Date separators are centered pills.
   - Reply preview and reactions render inside/under the bubble with a colored left bar using the original sender color.
5. **Room list like Telegram's chat list.** Rows: avatar, name, last-message preview, time, unread badge (muted rooms grey). Filter chips/tabs above the list (All, Unread, Direct, Rooms, + user-defined). Search field with a keyboard hint; the command palette (Cmd/Ctrl+K) is the same search.
6. **Composer** is a single rounded field with attach, emoji, and send/voice affordances that appear contextually. No row of toolbar buttons.
7. **Customizable chat background** (solid / gradient / subtle pattern, per theme, opt-in) is a first-class customization token (`--alloy-chat-background`), default plain surface. Patterns must not reduce text contrast below AA.
8. **Less chrome, more content.** Single typeface stack from tokens, 2-3 text sizes in chrome, no borders where spacing/elevation suffices, reserved accent color for primary actions and own messages only.

## Alternatives considered
- Keep Discord-style gutter as default: dense, but weaker identity in 1:1 chats; kept as option.
- Icon-rich navigation: faster to scan for some users, but conflicts with the calm goal; labels first.

## Consequences
- alloy-theme gains tokens: `color.user.1..12`, `chat.background.*` (additive; schema v0 stays compatible).
- UI work (shell, timeline, composer, onboarding/settings) must follow this record; reviewers reject emoji icons and decorative icons.
- Accessibility: color is never the only sender indicator (name text always present).
