---
id: adr-0009-android-push
type: architecture-design
status: draft
title: "ADR 0009: Android background push notifications via UnifiedPush"
parent: alloy-architecture
tags: [adr, android, push, privacy]
---

# ADR-0009: Android Background Push Notifications via UnifiedPush

## Context

Alloy is a privacy-focused Matrix client targeting Android and desktop. To deliver a user experience on par with commercial messaging apps (Messenger, Telegram), users must be notified of incoming messages even when the app is not running.

Android's primary push service is Google Cloud Messaging (GCM/FCM), but Alloy's zero-telemetry principle and architecture exclude proprietary Google services. The Matrix ecosystem standardized on **UnifiedPush**, a multi-vendor push infrastructure allowing app developers and users to choose their own push distributor (e.g., ntfy.sh, NextPush) without lock-in.

## Decision

Implement Android background push via:

1. **UnifiedPush Connector (3.3.5)**: The official, audited library for registering with distributors and receiving push events.
   - Replaces the deprecated 2.x `MessagingReceiver` API with the modern `PushService` abstraction.
   - All distributor communication is handled by the connector; Alloy registers a single HTTP pusher endpoint.

2. **Rust Push Handler (`crates/alloy-push`)**: Headless event processing on the backend:
   - `fetch_notification(payload)`: Decrypts Matrix push payloads (including E2EE) using the persisted session, without UI interaction.
   - `set_pusher` / `remove_pusher`: Manages the HTTP pusher registration with the homeserver via the Matrix `/api/v3/pushers` endpoint.
   - **Payload format**: `event_id_only` (fetch full event details on client-side post-notification; distributors never see message content).

3. **Android JNI Bridge (`crates/alloy-ffi`)**: Exports Rust functions to Kotlin via `System.loadLibrary("alloy_app_lib")`:
   - Loads on a background thread in `PushNative` to avoid ANR.
   - Error handling: catches panics and returns JSON error responses.

4. **Push Service (`AlloyPushService : PushService`)**: Receives distributor events:
   - `onNewEndpoint()`: Runs gateway discovery, registers HTTP pusher with homeserver.
   - `onMessage()`: Foreground check (via `ProcessLifecycleOwner`); if backgrounded, decrypts and posts notification.
   - `onUnregistered()`, `onRegistrationFailed()`, `onTempUnavailable()`: Standard lifecycle handlers.

5. **JS Interface (`window.AlloyPush`)**: UI control and state synchronization:
   - `state()`: Returns JSON (`{"enabled":bool,"registered":bool,"distributor":string|null,...}`).
   - `enable()` / `disable()`: User toggles push on/off.
   - `pick()`: Let user select an alternative distributor.
   - `pendingRoom()` / `takePendingRoom()`: Fetch and clear the room ID from notification intent.
   - State changes broadcast via `window.dispatchEvent(new CustomEvent('alloy-push-state', {detail: <state>}))` on the UI thread.
   - Pending room dispatch via `alloy-open-room` event after page load.

6. **HTTP Pusher Gateway**: Default gateway `https://matrix.gateway.unifiedpush.org/_matrix/push/v1/notify` (per Matrix spec); endpoint origin can override via gateway discovery (`GET /_matrix/push/v1/notify`).

## Rationale

- **UnifiedPush compatibility**: Aligns with the broader open-source ecosystem; users are not locked into a single distributor.
- **E2EE support**: Headless Rust decryption preserves privacy; push gateway never sees plaintext.
- **No proprietary services**: Zero Google/FCM, zero analytics, zero telemetry.
- **Foreground suppression**: `ProcessLifecycleOwner` lifecycle state prevents duplicate notifications when the app is open (sync updates are already live).
- **Same session data as the engine**: the push handler opens the account in the app data directory (`context.dataDir`, the value Tauri's `app_data_dir()` returns on Android: where `accounts.json` and `secrets.json` live), not `filesDir`.

## Limitations

- **Distributor dependency**: Requires a distributor app (e.g., ntfy client, NextPush) to be installed. If absent, UI will inform user and link to https://unifiedpush.org/users/distributors/.
- **FCM path deferred**: v0.2.1 does not implement FCM as a fallback. A future v0.3.0 or later may add FCM push via a separate code path (conditionally compiled).
- **Message preview**: the web UI copies "Show message preview" to the Android side (`AlloyPush.setPreview`, kept in SharedPreferences `alloy_push`). With it off, notifications posted while the app is closed show the sender or room but only say "New message". The setting reaches the native side when the app starts and whenever it changes, so a change made on another device applies after the next time the app opens here.
- **Back from a tapped notification**: when a notification starts the app from nothing, the chat opens but Back may leave the app instead of returning to the chat list; the browser engine skips history entries added before the first touch.
- **Single instance**: Currently supports one registration (`instance = INSTANCE_DEFAULT` in UnifiedPush). Multi-account support deferred.

## Implementation Notes

### Kotlin Classes

- **`PushNative`** (`object`): JNI wrapper, lazy-loads `liballoy_app_lib.so`.
- **`AlloyPushService`** (`: PushService`): Receives endpoints and messages; runs Rust handlers on background thread.
- **`PushBridge`** (JS interface): Mounted in `MainActivity.onWebViewCreate()`.
- **`MainActivity`**: Handles `room_id` intent extra from notification tap; queues or dispatches `alloy-open-room` event.

### SharedPreferences (`alloy_push`)

- `enabled`: bool (user toggle)
- `endpoint`: string (distributor-provided push URL)
- `gateway`: string (HTTP gateway URL for pusher)
- `error`: string (last error, if any)

### Manifest

- `<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />` (API 33+, runtime-requested on first enable).
- `<service android:name=".AlloyPushService" android:exported="false">` with intent-filter for `org.unifiedpush.android.connector.PUSH_EVENT`.
- Notification channel `messages` (IMPORTANCE_HIGH) created at app startup.

### Notification IDs

- Per-room grouping using `roomId.hashCode().absoluteValue` as notification ID.
- Group summary ID = 0.
- Small icon: `ic_launcher` mipmap or `ic_launcher_foreground` if available, else `android:drawable/ic_dialog_info` placeholder.

### UI Initialization (push.ts)

1. On app startup, listen for `alloy-push-state` custom event.
2. After page load, call `window.AlloyPush.pendingRoom()` to check for notification launch.
3. If not empty, dispatch `alloy-open-room` manually (fallback; Android/MainActivity may have already done so).
4. Render Settings > Notifications with capability guard `backgroundPush`.

## Trade-offs

| Aspect | Choice | Alternative | Notes |
|--------|--------|-------------|-------|
| Distributor model | Multi-vendor (UnifiedPush) | FCM only | Defers FCM; users choose distributor. |
| Push format | `event_id_only` (decrypt client-side) | `data` format (decrypt server) | `event_id_only` is simpler for headless, but requires sync to fetch full event; tradeoff accepted. |
| Notification appearance | System native (`NotificationCompat`) | Custom UI | System notifications are more reliable and respect user settings (silent, vibration, etc.). |
| Gateway URL | Default `matrix.gateway.unifiedpush.org`, configurable via discovery | Hardcoded per-distributor | Discovery allows flexibility; fallback is safe default. |

## Acceptance Criteria

- [ ] Kotlin compiles without errors (`./gradlew :app:compileArm64DebugKotlin`).
- [ ] Manifest merges successfully (`./gradlew :app:processArm64DebugManifest`).
- [ ] `PushNative` methods are declared (verified via `nm` on compiled `liballoy_app_lib.so`).
- [ ] Push state (enabled/disabled) persists across app restarts.
- [ ] `alloy-open-room` event fires when notification is tapped.
- [ ] Unencrypted and encrypted (E2EE) messages both produce notifications with decrypted text.
- [ ] Foreground suppression: message received while app is open does **not** generate duplicate notification.
- [ ] No analytics or telemetry in push flow.

## Related

- Issue #24 (Android background push).
- `local://push-contract.md` (shared contract; implementation follows this exactly).
- Rust crates: `crates/alloy-push`, `crates/alloy-ffi`.
