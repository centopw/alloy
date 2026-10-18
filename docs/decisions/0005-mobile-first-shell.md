# 0005: A real mobile UI, not a scaled-down desktop

Status: accepted. Extends 0002 to 0004.

## Problem
Below 900px the app was the desktop shell squeezed narrow: a permanent 72px rail beside the chat list,
22 to 33px controls, floating desktop cards for room info, hover-only message actions, no Send button, centred
dialogs, and a page the user could drag around (the shell scrolled).

## Decision
- **Phone = width <= 899px** (`stores/viewport.svelte.ts`). One screen at a time: Chats (title bar, space chips
  only when spaces exist, room list) with a bottom tab bar (Chats, Discover, Settings); a chat opens full screen
  with a back arrow; room info, members, threads and settings are full-screen pages; dialogs are bottom sheets;
  context menus are action sheets. Android Back unwinds the same stack.
- **The page never scrolls.** `html`/`body` are fixed and `overflow: hidden`; only lists and pages scroll;
  the shell applies safe-area padding itself.
- **Touch first.** Every interactive element is at least 44x44 (switches via a 44px hit area), inputs use
  16px text, hover-only affordances are replaced by long-press, the composer has a Send button and a `+`
  sheet for attachments and polls, and phones default to the bubbles message layout.
- **Guarded by a test.** `E2E_MOBILE=1 node scripts/e2e-bridge.mjs` drives the real app at 390x844 with touch
  emulation and fails (with `E2E_STRICT=1`) on page scroll, horizontal overflow, or any target under 44px.

## Consequences
Desktop is unchanged. Screens added later must pass the mobile audit. Not done: swipe gestures, pull to
refresh, and per-device haptics.
