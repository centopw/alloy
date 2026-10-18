# 0004: Settings configure behaviour (Element as reference)

Status: accepted. Extends 0002 and 0003.

## Problem

Settings had six pages and almost nothing to configure: a theme switch, a density control, and notes saying
features were "not available yet". Element's settings (Account, Devices, Appearance, Notifications,
Preferences, Keyboard, Sidebar, Security & Privacy, Encryption, Help & About) are the benchmark: each control
changes how the app behaves.

## Decision

1. **Rule: no dead controls.** A setting ships only if something reads it. Settings Element has that Alloy
   cannot honour yet (voice and video, labs, spell check, language, identity server, integrations, link
   previews, email/phone) are left out instead of faked.
2. **Two kinds of setting, one place each.**
   - *Account-wide* settings live on the server and apply on every device: notification rules (push rules:
     default level for one-to-one and group rooms, `@room` and name mentions, invitations, per-room level),
     display name, avatar, room notes. The engine exposes them as commands.
   - *Device* settings live in the `prefs` store (`stores/prefs.svelte.ts`): a typed, validated key list,
     persisted to localStorage for instant first paint and to the engine settings file, with unknown keys and
     invalid values dropped on load.
3. **Pages** follow Element's grouping: User settings (Account, Devices, Security & Privacy, Encryption),
   App settings (Appearance, Notifications, Preferences, Keyboard, Sidebar), Info (Help & About). Controls come
   from one small kit (`components/settings/controls`) so every page looks and behaves the same.

## What each setting changes

| Setting | Effect |
|---|---|
| Message layout (Modern, Bubbles, IRC) | Re-renders message rows in all timelines |
| Font size | Scales the whole UI (root font size) |
| Time format | Every timestamp (message rows, room list) |
| Big emoji | 1 to 3 emoji-only messages render large |
| Desktop notifications, sound, preview, only when unfocused | The notifier fires OS notifications when a room's notification count rises |
| Default level, mentions, invitations | Server push rules |
| Per-room level (room menu) | Server rule for that room; muted rooms show a bell-off icon and never notify |
| Enter to send | Enter sends, or Enter inserts a newline and Ctrl/Cmd+Enter sends |
| Send / show typing notifications | Typing events out and the typing indicator |
| Show join/leave, profile changes, room updates, removed messages | Timeline filtering; state events now read as sentences ("alice joined") |
| Jump to bottom on send | Timeline scroll behaviour |
| Autoplay GIFs and videos | Inline media playback |
| Room list previews, sections, People, Home shows all rooms | Sidebar structure and Home scope |
| Open at login | OS login item (desktop) |
| Display name, avatar, devices, session keys, clear media cache | Engine commands |

## Consequences

- New engine commands: `GetNotificationSettings`, `SetDefaultNotificationLevel`, `SetMentionRules`,
  `Get/SetRoomNotificationLevel`, `GetOwnProfile`, `SetDisplayName`, `SetOwnAvatar`, `ListDevices`,
  `GetSessionInfo`, `ClearMediaCache`. An account-wide default of "muted" is not offered because push
  rules cannot express it.
- The SDK's notification settings object is kept for the life of the client: a fresh one reads rules that
  only update on the next sync.
- Android release builds load the system CA certificates themselves (no JNI), because the platform
  verifier the HTTP stack defaults to aborts the process when uninitialised.
- Not done: key export/import, deleting other sessions, keywords, per-device notification toggles on
  Android (no push yet), keyboard rebinding.
