---
id: adr-0008-chat-parity
type: architecture-design
status: draft
title: "ADR 0008: Chat parity with Messenger and Telegram"
parent: alloy-architecture
depends-on: [adr-0001-ui-visual-language, adr-0007-design-spec-menus-emoji-updates]
tags: [adr, chat, features]
---

# ADR 0008: Chat parity with Messenger and Telegram

Status: accepted (0.2.0). Extends ADR 0001 (visual language) and ADR 0007 (menus, emoji, updates).

Alloy follows the everyday conversation features of Messenger and Telegram wherever Matrix has a carrier for them. Every
feature below is an ordinary Matrix mechanism, so Element and other clients see the same rooms and messages.

## Decisions

| Feature | How it works | Notes |
|---|---|---|
| Quick reactions | Hover toolbar (desktop), floating pill beside the pressed message (phone), double tap on a phone reacts with the first emoji | Double tap is phone-only; desktop double click keeps selecting text |
| Swipe right to reply | Phones only, 64 px threshold, ignored within 24 px of the screen edge (system back gesture) | Needs `touch-action: pan-y` on the row |
| Who reacted | Chip tooltip and a sheet with one tab per emoji | Data was already in `ReactionSummary.senders` |
| Sent / seen | One check = sent, two = read by someone else; seen avatars under your newest message in rooms of 20 or fewer | A receipt on any later event counts as read (`readersByEvent`). Setting: Show who has read your messages. Timelines now track receipts (`TimelineReadReceiptTracking::MessageLikeEvents`) |
| Jump to latest / mention | Badge with the number of unseen messages; an `@` button for mentions that arrived while scrolled up | `mentions_me` comes from the SDK's push-rule highlight |
| Pinned messages | `m.room.pinned_events`; bar under the header cycles newest to oldest, list sheet | Permission from power levels (`can_pin`) |
| Forward | Re-sends the content (relations and reply fallback stripped); Matrix has no forward event | Several messages and several rooms; polls cannot be forwarded |
| Delete | Matrix redaction (always for everyone) after a confirmation; moderators may delete others' messages | `can_redact_others` |
| Multi-select | Select mode with Copy, Forward, Delete | Android Back leaves it |
| Pinned chats | `m.favourite` room tag, shown in a Pinned section | Element shows them as Favourites |
| Archive | `m.lowpriority` room tag; own Archive chip; archived rooms stay out of every other list | A new message does not unarchive (tags are explicit) |
| Mark as unread | `m.marked_unread` account data | Opening the chat clears it |
| Mute for a time | Mute level plus a synced preference `mutedUntil` (`chat.alloy.prefs`); every client with the app open restores the room at expiry | Matrix push rules have no expiry. A mute only ends while some Alloy is running |
| Row actions | Hover "more" button (desktop), swipe left for Mute / Archive (phone), long swipe archives | Same gesture core as swipe-to-reply (`platform/swipe.ts`) |
| Drafts | The SDK's composer draft store; the list shows `Draft:` | Per room, restored after restart; not synced across devices |
| Albums | Captionless pictures from one sender within 60 s fold into a grid in the timeline (UI only, each stays its own event) | Gallery MSC4274 is unstable and not used |
| Media gallery | Swipe between pictures and videos, pinch and double tap to zoom, swipe down to close | |
| Voice messages | `m.audio` with `org.matrix.msc3245.voice` and a waveform (MSC3246); hold to record, slide left to cancel, slide up to lock | Needs `MediaRecorder`; the mic is hidden where it is missing. Recorded in the web layer, written to a temp file by the shell, uploaded by the engine |
| Audio player | Inline waveform player with 1x / 1.5x / 2x | Replaces the full-screen audio viewer |
| Spoilers | `\|\|text\|\|` becomes `<span data-mx-spoiler>`; blurred until clicked | Code spans are left alone |
| Formatting | Selection toolbar, "Aa" row on phones, Ctrl/Cmd+B, I, Shift+X, M, P, Q, L | Ctrl+K stays the room switcher, so links use Shift+L |
| `@room` | Picker entry for users who may use it; sets `m.mentions.room` | |
| Location | `m.location` with a `geo:` URI; the card links to OpenStreetMap. No map tiles are loaded in the app | Permissions: Android `ACCESS_FINE_LOCATION`, macOS usage string |
| Live location | Beacon (MSC3489): start, one beacon every 30 s, stop; banner with Stop | Incoming shares render as a card with the end time |
| Sessions | Sign out one or all other sessions with the account password (user-interactive auth) | |
| Moderation | Change role, remove, ban, unban from the member menu, gated by power levels; Report on messages | |
| Blocking | `m.ignored_user_list`; Settings > Security and privacy lists and unblocks | |
| Search everywhere | Sidebar search also searches message text on the server | Encrypted rooms are not covered; the list says so |
| Android download | `DownloadBridge` (MediaStore on API 29+, public Downloads below) exposed to the WebView as `AlloyAndroid` | Only files inside the app's own `media_cache` are accepted |

## Not built

No Matrix carrier, or excluded by an earlier decision: voice and video calls (post 1.0), scheduled messages (no stable API in
matrix-sdk 0.19), disappearing and view-once media, paid or star reactions, view counts, per-chat wallpapers, word effects,
stories, bot commands, a "delivered" state (Matrix has sent and read only) and silent send.
