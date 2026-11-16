---
id: adr-0018-timestamps-and-chat-backgrounds
type: architecture-design
status: accepted
title: "ADR 0018: Telegram-style inline timestamps and custom chat backgrounds"
parent: alloy-architecture
depends-on:
  - adr-0001-ui-visual-language
  - adr-0013-shadcn-chat-design-language
  - adr-0014-flush-light-material-shell
  - adr-0015-a11y-contrast-tokens
tags: [adr, ui, chat, timestamps, backgrounds, personalisation]
---

# 0018: Telegram-style inline timestamps and custom chat backgrounds

- Status: accepted (2026-11-16)
- Issue: #99 (chat parity)
- Date: 2026-11-16
- Supersedes: the always-below timestamp of ADR 0013's bubbles layout; the plain-canvas conversation surface

## Context

Two things in the conversation surface did not match the networks we mirror.

**Timestamps.** Every bubble put its time on a line of its own under the text, so a two-word reply
occupied two lines and the column grew: a screenshot of five short messages wasted about a third of
its height on time rows. Telegram and Messenger put the time *after* the last line when it fits
there, and only drop it to its own line when the text fills the line or the bubble carries reaction
chips (chips occupy the bottom edge, so the time sits below them).

**Backgrounds.** Messenger lets a user set a picture behind a conversation, per contact and per
group. Alloy's conversation surface was a flat token colour, which makes the app feel less personal
than the client it replaces, and it is the most requested personalisation for a chat client.

## Decision

**The timestamp belongs to the last line of the message.** It renders inline at the end of the last
text line when that line has room for it, right-aligned, after a 4px gap; otherwise it renders on
its own line below the text. A bubble that carries reaction chips always puts the time below the
chips. "Has room" is decided by measuring the last line — never by counting characters — because the
body may be rich text, emoji or a different font, and the answer must hold for every locale.

The measurement is CSS-state only: the row sets a flag and CSS owns the look, so a resize, a layout
or density change re-measures and nothing is positioned by inline styles that could drift.

**Backgrounds are per conversation, with a default.** A chosen image is copied into the app's own
media directory and referenced by file name, not by an absolute path: the asset protocol already
serves that directory, and the copy keeps the picture after the user moves the original. A
`__default` entry applies to every conversation without its own; a conversation can be cleared back
to the default. The picture sits behind the timeline with a token-derived scrim, and bubbles keep
opaque surfaces, so the contrast guarantees of ADR 0015 hold over any image.

## Alternatives considered

- **Timestamp always inline (float right).** Shortest bubbles, but the time overlaps the last word
  when the line is full, and floats cannot be pulled back into a line that has no room. Rejected
  without measurement.
- **Timestamp always below (status quo).** Predictable, but it doubles the height of short messages
  and does not match either network being mirrored.
- **Hide the timestamp until hover.** Fewer pixels, but the time is information, not chrome; hiding
  it also made the row grow on hover in an earlier iteration (ADR 0013).
- **Backgrounds as a theme-level setting only.** Cheap, but Messenger's model is per conversation,
  and a photograph behind every chat is not what a user who set one for a single contact asked for.
- **Store background by absolute path.** No copy, no disk use, and the picture disappears when the
  file moves. Rejected: a pref must not point into a directory the app cannot control.

## Consequences

- `MessageRow.svelte` measures the last text line with a `Range` and sets `time-inline`; both modes
  share one timestamp component so sent/read ticks and colour tokens stay identical.
- Reaction chips force the block mode, and the quick-reaction bar and action toolbar only reveal on
  pointer hover or *visible* keyboard focus: a click that leaves focus inside a bubble no longer
  pins the bar on screen.
- Backgrounds are stored under `<appData>/media/backgrounds/` and copied through a Tauri command;
  the pref holds the file name. Removing one deletes the file only when no conversation still
  references it.
- The scrim strength is a token, so a theme can tune legibility without touching components.
- The transport offers no background of its own: this is a local, per-device preference, and nothing
  about it is sent to the network.

## References

- ADR 0001 (visual language), ADR 0013 (bubbles), ADR 0015 (contrast tokens).
- Telegram and Messenger conversation surfaces, used as the reference models.
