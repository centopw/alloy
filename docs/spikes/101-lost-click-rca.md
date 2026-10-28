# #101 — why the CI E2E loses a chat-row click

Status: investigation closed with a bounded retry; the next failure now prints its own diagnosis.
Date: 2026-10-28. Owner: FE. Scope: `apps/alloy/ui/scripts/e2e-bridge.mjs`.

## Question

On CI, `openRoom('From Bob')` right after the forward dialog closed left no chat open, and a
four-click retry hid it. Was the click swallowed, was the header slow, or was the wrong row clicked?

## What the code says (verified)

1. **`el.click()` cannot be blocked by an overlay.** The helper clicks the row programmatically
   inside the page, so a still-closing dialog backdrop — the cause named in the old comment — cannot
   intercept it. Hit-testing only applies to real pointer events.
2. **The header is synchronous with the click.** `Sidebar.openRoom()` calls
   `roomsStore.select(roomId)`; `rooms.active` is a `$derived` lookup of that id in `rooms`, and
   `AdaptiveShell` renders `ChatHeader` from `active && activeId` in the same tick. If the click
   handler ran, `button[aria-label="Room info for …"]` exists on the next frame. A missing header
   therefore means **the handler did not run, or a different row was clicked** — not "rendering was
   slow".
3. **The old helper could click more than once per call.** `clickRow` clicked inside
   `page.waitForFunction`, whose predicate is re-evaluated every animation frame until it returns
   truthy. Every evaluation clicked again, so one `openRoom` attempt could deliver 2–3 clicks and
   the retry loop multiplied that. Re-clicking the already-open room was harmless, which is exactly
   why the failure was invisible.
4. **The retry could not tell the two outcomes apart.** A 4 s header timeout is also short enough to
   expire on a loaded CI runner even when the click did open the room; the next attempt then
   re-clicked an open chat and passed. So "four clicks needed" never proved a lost click.

## Reproduced locally (different path, real bug)

A real pointer click on a room row immediately after the shell paints is swallowed by the
**permissions gate backdrop**: `PermissionsGate` opens once `permissionsStore.loaded` flips after its
first refresh, so the modal appears *after* the shell is already interactive (measured at 1280×900
against the dev engine: the click at t=0 lands on `.backdrop`, the same click at t≥100 ms opens the
chat). This cannot explain the CI failure — the script clicks programmatically — but it is a genuine
lost click for a real user on first run. It is left as-is here because the gate is modal by design and
dismissing it is the intended first action; the harness already dismisses it before interacting.

## Decision

Keep a retry, but make it bounded and self-diagnosing:

- one click per attempt (the row is found and clicked in one page task, which keeps the
  mid-update protection without repeated clicks);
- two attempts with an 8 s header wait each (≤16 s to fail, instead of 4 × 4 s of silence);
- on a failed attempt, print the page state: open dialogs, every matching row with its
  `data-room-id` / `aria-current`, the current room, whether the header exists, whether `.chat` is
  mounted. `data-room-id` was added to the row button for this;
- the thrown error names the diagnostic line.

If #101 recurs, that line distinguishes the remaining cases: `currentRoom` changed but no header
(handler ran, header did not render — a UI bug), `currentRoom` unchanged (click did not reach the
handler or hit the wrong row), or `matchingRows` empty (the list no longer had the row).

## Residual risk

The root cause of the single CI occurrence is still not proven — the run's page dump was not kept.
This note replaces an unverified comment with verified mechanics and makes the next occurrence
self-documenting, which is what #101 asked for. Do not remove the second attempt until a CI run has
passed with the diagnostic in place and the dump has been read.
