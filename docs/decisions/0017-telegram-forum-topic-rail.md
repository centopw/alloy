---
id: adr-0017-telegram-forum-topic-rail
type: architecture-design
status: accepted
title: "ADR 0017: Telegram forums open into a conversation with a topic rail"
parent: alloy-architecture
depends-on:
  - adr-0001-ui-visual-language
  - adr-0002-discord-style-layout
  - adr-0016-on-device-connectors
tags: [adr, ui, telegram, forums, topics]
---

# 0017: Telegram forums open into a conversation with a topic rail

- Status: accepted (2026-11-15)
- Issue: #125 (epic: on-device connectors)
- Date: 2026-11-15
- Supersedes: the full-page topics list introduced with forum support in 0.4.2

## Context

A Telegram forum supergroup is a container whose topics are conversations of their own. Alloy
models each topic as a chat with `parent_remote_chat_id` set (ADR 0016), and the first UI for that
model put the forum's topics on a page of their own: selecting the forum replaced the conversation
with a list titled "Topics in …", and a topic opened from there.

Two problems came out of real use at a forum with 29 topics:

1. **It took two clicks to reach any conversation, and the list was the whole pane.** A user who
   wanted to read one topic paid for the list every time, and the list pushed the conversation out
   of the window entirely.
2. **While reading, the other topics were invisible.** Switching conversation meant going back to
   the list, which is the same navigation problem the list was meant to solve.

Telegram's own client solves it differently: opening the forum lands you in a topic (General, the
most recently active one) and a narrow vertical rail of topics sits beside the messages, so reading
and switching never leave the conversation. That is the model users already know from the network
being mirrored.

## Decision

A forum is a **container that opens into a conversation**, and topics are switched from a rail
beside it.

- **One click to a conversation.** Selecting a forum opens its most recently active topic (newest
  activity, ties by name); a forum with no topics opens as an empty state with its name.
- **The rail is ordered by the conversation that moved last.** Topics are conversations of their
  own, and the rail sorts them by the newest activity the chat itself reports — the network's
  "last message" timestamp, not merely the newest message stored on this device. A topic whose
  history has not been fetched yet still sorts where the network says it belongs, and a message
  arriving in any topic moves it to the top immediately. The forum's row in the chat list previews
  that same newest topic's message; the topic count stands in only until a topic has history.
- **The rail.** A narrow vertical column at the leading edge of the conversation pane lists the
  forum's topics: a circular initials badge coloured from the same palette as every other avatar
  (ADR 0001), a short two-line label, the unread count, a muted marker, and the active topic
  highlighted. It scrolls on its own and stays put while messages scroll.
- **Phones get a strip.** The same data renders as a horizontal chip strip above the header, where a
  vertical column would eat the width a phone does not have.
- **The chat list shows one row per forum.** No inline tree, no chevron: the row carries the forum's
  summed unread count and topic count, is highlighted while any of its topics is open, and clicking
  it opens a topic rather than expanding anything.
- **Keyboard.** Arrow keys move within the rail (or the strip), Enter/Space opens, and the roving
  tabindex in the chat list is unchanged.

## Alternatives considered

- **Keep the full-page list, add a back button.** Cheapest change, and it keeps the list's
  readability for very long topic names. Rejected because it keeps the navigation cost: the
  conversation and the topic switcher are never on screen together.
- **Groups and channels alike in the rail.** Reusing the rail for ordinary group members was
  considered and rejected: members have roles and presence, topics have unread counts and are
  rooms in their own right. One surface, one meaning.
- **A dropdown in the chat header.** One control, no layout cost, but it hides the topics behind a
  popup and shows nothing about unread state until opened. Rejected: the rail's value is that
  unread topics are visible at a glance.
- **Tabs across the top.** Familiar on desktop, but a forum has up to hundreds of topics and tabs do
  not scale past a handful; the header also already carries the title, search and info actions.

## Consequences

- `TopicsRail.svelte` owns the rail and the phone strip; the conversation pane composes it beside
  the header, timeline and composer.
- `TopicsPane.svelte` and its strings are gone. The i18n gate keeps them from coming back unused.
- The rail duplicates the forum's topic list that the engine already delivers as rooms, so it costs
  no extra engine call; topics are ordinary rooms with `parent_id`.
- A forum with many topics needs the rail's own scroll; the active topic must be scrolled into view
  when it changes, which the rail does on selection.
- The chat list loses inline expansion. Anyone who wants the tree can still find every topic in the
  rail, and the sidebar stays a list of conversations, which is what a chat list is for.
- Search still finds a topic by name and opens it directly; the rail then shows where it sits.

## References

- ADR 0001 (visual language: tokens, avatar palette), ADR 0002 (shell layout), ADR 0016 (on-device
  connectors, topic identity as `{forum}/t{topic}`).
- Telegram's own forum UI, used as the reference model.
