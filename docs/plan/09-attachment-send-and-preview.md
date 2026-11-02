# 09 — Attachment send fix and Telegram-style preview modal

Status: agreed scope, in progress.
Source: user report 2026-11-02 — "fix the problems where we can't sent a media files" plus a request for
Telegram-style file preview before sending.

## Problem

1. Dropping a file into the composer and sending fails with the toast
   `attachment /Users/.../BC_BSC_10_2026_NGUYEN_TAN_HIEP.pdf is not registered for sending`.
2. The pre-send attachment UI is a single thin composer strip (icon, truncated name, `X`,
   "Press Enter to send", "Send attachment"). The user wants the Telegram flow instead: a modal that
   previews the file and lets it be sent alone or with a caption.

## Confirmed root cause (research, 2026-11-02)

- The message is produced in the engine, not the UI. Four sites call `ctx.attachment_path_allowed(&path)`
  and return `EngineError::forbidden("attachment {file_path} is not registered for sending")`:
  - `crates/alloy-engine/src/handlers/media.rs:90` and `:141`
  - `crates/alloy-engine/src/handlers/rooms.rs:234`
  - `crates/alloy-engine/src/handlers/auth.rs:548`
- The allowlist is `AttachmentAllowlist` in `crates/alloy-engine/src/context.rs:11-107`:
  a `HashSet<PathBuf>` of individual files plus a `Vec<PathBuf>` of directory roots, keyed by
  canonicalized absolute path, held in memory for the process lifetime, with no expiry or cleanup.
- Registration happens **only** inside Rust commands:
  - `pick.rs:246` → `allow_attachment_path` for each file the native picker returns
  - `pick.rs:134` and `pick.rs:272` → `allow_attachment_root` for the pick cache directory
  - `lib.rs:76` → registers `temp_voice/` at startup (why voice messages are unaffected)
- The defect: `apps/alloy/ui/src/lib/components/composer/Composer.svelte:369-375` passes the paths from
  Tauri's `onDragDropEvent` straight into `stagePaths()`, which calls `stageFile()`
  (`composer/stageHelper.ts:37-44`) and only builds a `StagedPicture`. No command registers the path.
  The send chain `sendStaged()` → `timelineStore.sendFile()` (`stores/timeline.svelte.ts:276`) →
  `client.sendAttachment()` (`client.ts:295`) → engine `SendAttachment` therefore fails the check.
- Blast radius: **all** file types dropped by drag. The picker path registers correctly. Paste works
  because `save_pasted_image` / `import_pasted_image_path` register the path. Voice is unaffected.

## Part 1 — the fix (blocking)

Make drag-drop sending work for every file type.

- Follow the existing precedent of the paste import command (`import_pasted_image_path` on the Rust side,
  wrapped by `importPastedImagePath` in `platform/files.ts`): add an equivalent **drop-import** entry point
  used by the drag handler, so registration stays on the Rust side with the same validation and
  canonicalization semantics.
- Do **not** introduce a bare primitive that registers any path the webview names while skipping the checks
  the picker and paste commands already perform. The allowlist is a security boundary; validation must be at
  least equivalent to the existing import command.
- Keep the change minimal: no allowlist redesign, no expiry/cleanup, no change to engine error semantics or
  message text.
- Re-confirm that the picker and paste flows still register and send after the change.

## Part 2 — preview modal (decisions fixed and approved by the requester)

- **One** modal for every attach entry point: file picker, drag-drop, and paste.
- Images and video: large visual preview. Every other type: a file card with icon, name and human-readable
  size.
- Header: file count label with correct singular/plural, close control on the left, overflow menu on the
  right.
- Footer: caption input with placeholder `Add a caption...`, emoji button, send button.
- Multiple files page through with an obvious counter or navigation.
- Sending must work **with** a caption and **without** one.
- Cancelling must leave the composer sane: no orphaned staged files, no stuck overlay, draft text preserved.
- **No** compression toggle and no "without compression" wording. Research confirmed that string is not in
  this repo; it came from the Telegram reference screenshots.
- `UploadStrip.svelte` stays as-is — it renders in-progress uploads, a separate concern.

### Constraints

- Reuse `common/Modal.svelte` for backdrop, focus trap, Tab cycling, Escape close, phone bottom-sheet,
  `role="dialog"` and `aria-modal`. Do not fork a new dialog primitive. If the default 520px width is too
  narrow for media, add an optional width/size prop to `Modal.svelte` and keep every existing call site working.
- Follow composer conventions: `$state` for local component state, imported stores called as methods,
  `StagedPicture` (`path`, `name`, `kind`, `preview`) as the data shape.
- Suggested contract: new `composer/AttachmentPreview.svelte` with props
  `{ items: StagedPicture[], onSend: (caption: string) => void, onCancel: () => void }`, opened by local
  `$state` in `Composer.svelte`, emoji via the existing `emojiPicker` store, reusing common `Button` and
  `TextField`. `sendStaged()` already attaches the caption to the first item on multi-file sends — keep that
  semantic.
- Accessibility is in scope, not a follow-up: focus lands inside the dialog, Escape cancels, every control
  has an accessible name, the file counter is announced, and send works with the keyboard alone.

## Part 3 — tests and verification

- Unit tests for what changes, including `composer/stageHelper.test.ts` and new modal logic.
- Rust: `crates/alloy-engine/tests/` holds `media_integration.rs`, `media_location_integration.rs`,
  `room_media_integration.rs`. Add a test proving that a path registered through the new drop-import entry
  point passes `attachment_path_allowed`, and that an unregistered path is still rejected. **Required** —
  this is a security boundary.
- Run `pnpm check` and `pnpm test` in `apps/alloy/ui`, plus the Rust tests for the crates touched. Report
  real output.
- E2E: project convention requires UI markup changes to keep e2e selectors in sync in the same change. The
  referenced `scripts/e2e-bridge.mjs` was **not found** by research (only `perf-desktop.mjs`,
  `perf-summary.mjs`, `ci-engine-shards.sh`). Locate the actual harness (search the repo for `e2e`, check
  `package.json` scripts and `.github/workflows/nightly.yml`) and update its selectors, or state with
  evidence that none exists.

## Non-goals

No compression option, no allowlist redesign, no server or protocol changes, no redesign of the rest of the
composer, no commit, no push.

## Acceptance criteria

1. A PDF, an image, a video and a multi-file drag-drop each send successfully from the desktop app, with and
   without a caption. The "not registered for sending" toast no longer appears for dropped files.
2. Picker and paste attachment flows still send.
3. An unregistered path is still rejected by the engine, with a regression test proving it.
4. Dropping, picking or pasting a file opens one preview modal showing the correct preview or file card,
   file count, caption input, emoji and send; multiple files page.
5. Escape and the close control cancel cleanly and preserve the composer draft.
6. Keyboard-only send works; dialog has proper roles and accessible names.
7. `pnpm check`, `pnpm test` and the touched Rust tests pass; e2e selectors updated or their absence proven.

## Open items / risks

- Whether the mobile (Android) drag-drop payload differs on desktop is unverified; mobile may simply not
  produce paths. Check before assuming the same code path works there.
- `Modal.svelte` width may need a variant for media; prefer an optional prop over a second dialog component.
- The e2e harness location is unresolved and the project memory about `scripts/e2e-bridge.mjs` may be stale.

## As built (2026-11-02)

Two resolutions differ from the draft above, both deliberate:

- **Registration is anchored to the OS event, not to a page-supplied path.** The draft suggested a
  `register_attachment_path(path)` command. That would have let any script in the web view name any file on
  disk and have the engine admit it — a wider surface than the picker, which needs a native dialog, and than
  paste, which is limited to picture extensions and a size cap. Instead Tauri's Rust-side
  `WebviewEvent::DragDrop` records the paths of each real drop into `DroppedPaths` (bounded, replaced per
  drop), and `take_dropped_attachments()` hands exactly that queue back after canonicalizing and admitting
  each file. The page never names a path, so it cannot admit a file the user did not drop.
- **`Modal.svelte` needed no width fork.** It already took a `width` prop; it gained three optional props
  (`headerStart`, `headerEnd`, `closable`) so the preview can put its close button on the left and its
  overflow menu on the right without a second dialog primitive. Existing call sites are untouched.

File size for the card comes from a new `attachment_size` command that answers only for paths already on the
allowlist, so it cannot be used to probe the filesystem.

Corrections to the research findings:

- The e2e harness exists: `apps/alloy/ui/scripts/e2e-bridge.mjs` (research looked in the repo-root `scripts/`).
  It drives the composer by aria-label but has **no** attachment/drag-drop coverage, and none of its selectors
  collide with the new modal, so no selector update was required.
- The worktree already carried an in-progress i18n refactor. Five pre-existing test failures
  (`message-actions` ×3, `sidebarTags`, `replyQuote`) assert literal English markup that refactor replaced;
  they are unrelated to this change and were left alone.

Still unverified: the end-to-end drop inside the built desktop app. The Rust and UI layers are covered by
unit tests, but a real Finder/Explorer drop needs a running Tauri build.

## Review round

An independent read-only review raised two blocking points, both resolved:

1. **The required allowlist test was missing.** The three queue tests proved `DroppedPaths` but not that a
   dropped file actually reaches the engine allowlist. Registration is now a standalone
   `register_dropped(ctx, dropped)` and is covered directly: a dropped file is admitted while its sibling in
   the same directory is not, a dropped **directory** admits nothing beneath it, a missing path admits
   nothing, and a dropped symlink admits the file it points at (the same thing the picker does).
2. **The strip and the modal disagreed about the caption.** The composer hint says "your text becomes the
   caption" while the modal started from an empty box, so the same draft meant two different things
   depending on which Send was pressed. The modal now opens with the composer draft as its caption and
   spends that draft when it sends, so there is one caption per send.

Non-blocking fixes taken in the same round: the drop queue became FIFO batches (two drops in one event-loop
turn no longer lose the first and raise a false error), `Modal.svelte` no longer steals focus from a dialog
that already chose where the keyboard goes, re-attaching an already-staged file reopens the preview, the two
shell round-trips per file run in parallel, and two comments that stated the opposite of the code were
corrected.

Deliberately left as follow-ups, with reasons:

- Escape with the emoji picker open closes both the picker and the dialog. Pre-existing, and
  `EmojiPickerHost.svelte` is mid-refactor by someone else; a one-line `stopPropagation` fixes it.
- Files beyond the 64-path cap, non-UTF-8 paths and directories are skipped quietly rather than reported.
- `Send ${n} attachments` in the strip is hardcoded English (pre-existing).
- New keys went to `en.ftl` only; `vi.ftl` is already partial.
- `formatBytes` has no GB tier, so a large video reads as a big number of MB.
