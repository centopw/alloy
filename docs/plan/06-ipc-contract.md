# 06 — IPC Contract & Transport Specification (Protocol v1)

## 1. Summary

This document specifies the frozen **Protocol v1** contract between the Rust core engine and the platform UI shells (Svelte 5 / TypeScript desktop/mobile shell and post-v1 web shell).

Protocol v1 establishes:
1. **Strongly-typed IPC boundary**: Tagged enums for commands (`EngineCommand`), responses (`EngineResponse`), events (`EngineEvent`), and errors (`EngineError`).
2. **VectorDiff reactive streams**: 11-variant differential list synchronization (`VectorDiffDto<T>`) matching eyeball-im streams from `matrix-sdk-ui`.
3. **Pluggable Router Architecture**: `RouterEngine` in `alloy-app` dispatching to per-module handler trait objects with explicit `EngineError::NotImplemented` for stubbed endpoints.
4. **Transport Abstraction**: Svelte 5 / TypeScript `Transport` interface (`TauriTransport` for runtime, `MockTransport` with fake data and simulated SAS/local-echo transitions for development/testing).
5. **Clear module ownership**: Disjoint files and directories assigned to parallel implementation streams.

---

## 2. Architecture & Dataflow

```mermaid
flowchart TB
  subgraph UI["apps/alloy/ui (Svelte 5 + TypeScript)"]
    direction TB
    Components[Components: AdaptiveShell / Timeline / Composer / HUD]
    Store[Runes State Stores: applyDiff]
    Client[AlloyClient typed facade]
    TransportInterface[Transport interface]
    TauriTransportImpl[TauriTransport]
    MockTransportImpl[MockTransport]

    Components --> Store
    Store --> Client
    Client --> TransportInterface
    TransportInterface --> TauriTransportImpl
    TransportInterface --> MockTransportImpl
  end

  subgraph IPC["IPC Boundary (Tauri 2 / IPC Channels)"]
    InvokeCmd["alloy_invoke(command: EngineCommand) -> Result<EngineResponse, EngineError>"]
    SubEvents["alloy_subscribe(on_event: Channel<EngineEvent>)"]
  end

  subgraph Backend["apps/alloy/src-tauri & crates/*"]
    Router[RouterEngine: Engine trait]
    AuthHandler[AuthHandler (core-auth)]
    RoomsHandler[RoomsHandler (core-rooms-timeline)]
    TimelineHandler[TimelineHandler (core-rooms-timeline)]
    CryptoHandler[CryptoHandler (core-crypto)]
    MediaHandler[MediaHandler (core-media)]
    SpacesHandler[SpacesHandler (core-spaces)]
    ThemeHandler[ThemeHandler (theme-runtime)]

    Router --> AuthHandler
    Router --> RoomsHandler
    Router --> TimelineHandler
    Router --> CryptoHandler
    Router --> MediaHandler
    Router --> SpacesHandler
    Router --> ThemeHandler

    CoreClient[AlloyClient (crates/alloy-core)]
    Storage[SecretStore (crates/alloy-storage)]
  end

  TauriTransportImpl -->|invoke| InvokeCmd
  TauriTransportImpl -->|channel| SubEvents
  InvokeCmd --> Router
  SubEvents --> Router
  AuthHandler & RoomsHandler & TimelineHandler --> CoreClient
  CoreClient --> Storage
```

---

## 3. Protocol Commands (`EngineCommand`)

Commands are serialized with `#[serde(tag = "action", content = "params")]`.

| Group | Command | Params | Response Payload (`EngineResponse`) | Description |
|---|---|---|---|---|
| **Auth** | `DiscoverHomeserver` | `{ input: string }` | `HomeserverInfo { url, name, supports_oidc }` | Discovers .well-known and validates homeserver endpoint |
| | `LoginPassword` | `{ homeserver_url, username, password }` | `LoggedIn { account }` | Logs in with username and password |
| | `StartOidcLogin` | `{ homeserver_url }` | `OidcUrl { redirect_url }` | Generates OIDC auth URL for OAuth 2.0 PKCE flow |
| | `CompleteOidcLogin` | `{ redirect_url }` | `LoggedIn { account }` | Completes OIDC authorization redirect |
| | `RestoreSession` | `{ account_id }` | `SessionRestored { account }` | Restores stored session from OS keychain |
| | `Logout` | `{ account_id }` | `LoggedOut` | Invalidates tokens and clears keychain entry |
| | `ListAccounts` | *(none)* | `AccountsList { accounts }` | Lists all stored local accounts |
| **Rooms** | `SubscribeRoomList` | `{ filter?: string \| null }` | `RoomListSubscribed { subscription_id }` | Starts sliding sync room list stream |
| | `Unsubscribe` | `{ subscription_id }` | `Unsubscribed { subscription_id }` | Cancels an active stream subscription |
| | `JoinRoom` | `{ room_id_or_alias }` | `RoomJoined { room_id }` | Joins a room by ID or alias |
| | `LeaveRoom` | `{ room_id }` | `RoomLeft { room_id }` | Leaves or forgets a room |
| | `CreateRoom` | `{ name, encrypted, invite? }` | `RoomCreated { room_id }` | Creates a new room with optional encryption |
| **Timeline** | `SubscribeTimeline` | `{ room_id }` | `TimelineSubscribed { subscription_id }` | Opens room timeline and streams diffs |
| | `PaginateBackwards` | `{ room_id, count }` | `Unit` | Fetches historical messages from homeserver |
| | `SendText` | `{ room_id, body, markdown?, reply_to?, mentions?, txn_id? }` | `MessageSent { room_id, txn_id, event_id }` | Sends message via send queue (generates local echo) |
| | `EditMessage` | `{ room_id, event_id, body, markdown? }` | `MessageEdited { room_id, event_id }` | Replaces message content with m.replace |
| | `RedactEvent` | `{ room_id, event_id, reason? }` | `EventRedacted { room_id, event_id }` | Redacts an event |
| | `ToggleReaction` | `{ room_id, event_id, key }` | `ReactionToggled { room_id, event_id, key, added }` | Adds or removes emoji reaction |
| | `RetrySend` | `{ room_id, txn_id }` | `SendRetried { room_id, txn_id }` | Retries a failed send transaction |
| | `CancelSend` | `{ room_id, txn_id }` | `SendCancelled { room_id, txn_id }` | Drops a queued or failed message |
| | `SendAttachment` | `{ room_id, file_path, mime_type?, txn_id? }` | `AttachmentSent { room_id, txn_id, event_id }` | Encrypts and uploads media attachment |
| | `MarkRead` | `{ room_id, event_id }` | `ReadMarked { room_id }` | Sends unthreaded read receipt |
| | `SetTyping` | `{ room_id, typing }` | `TypingStatusSet { room_id }` | Sets ephemeral typing notice |
| **Media** | `GetMedia` | `{ source, kind, width?, height? }` | `MediaRetrieved { local_path }` | Authenticated media download/thumbnail cache |
| **Crypto** | `RequestSelfVerification` | *(none)* | `VerificationRequested { user_id, device_id }` | Requests verification with other owned devices |
| | `AcceptVerification` | `{ user_id, device_id }` | `VerificationAccepted { user_id, device_id }` | Accepts incoming verification request |
| | `StartSas` | `{ user_id, device_id }` | `SasStarted { user_id, device_id, emojis }` | Starts short auth string (SAS) flow |
| | `ConfirmSas` | `{ user_id, device_id }` | `SasConfirmed { user_id, device_id }` | Confirms matching SAS emoji/decimals |
| | `CancelVerification` | `{ user_id, device_id }` | `VerificationCancelled { user_id, device_id }` | Aborts verification flow |
| | `GetRecoveryStatus` | *(none)* | `RecoveryStatusRetrieved { state }` | Returns backup key recovery state |
| | `EnableRecovery` | *(none)* | `RecoveryEnabled` | Enables server-side Megolm key backup |
| | `RecoverWithKey` | `{ recovery_key }` | `RecoveredFromKey { recovered }` | Restores keys from base58 recovery key |
| | `ResetRecovery` | *(none)* | `RecoveryReset` | Resets/rotates server-side secret storage |
| **Spaces** | `SubscribeSpaces` | *(none)* | `SpacesSubscribed { subscription_id }` | Subscribes to space hierarchy |
| | `SubscribeSpaceChildren` | `{ space_id }` | `SpaceChildrenSubscribed { subscription_id }` | Subscribes to rooms in a space |
| **Theme / Settings** | `ListThemes` | *(none)* | `ThemesList { themes }` | Lists available themes (dark, light, amoled, etc.) |
| | `GetTheme` | `{ id }` | `ThemeCss { css }` | Returns resolved CSS custom properties |
| | `SetActiveTheme` | `{ id }` | `ThemeSetActive` | Sets active theme |
| | `GetSettings` | *(none)* | `SettingsRetrieved { settings_json }` | Returns user preferences |
| | `SetSetting` | `{ key, value_json }` | `SettingSet { key }` | Saves user preference |

---

## 4. Reactive Events (`EngineEvent`)

Pushed from backend over `alloy_subscribe(Channel<EngineEvent>)`. Tagged with `#[serde(tag = "type", content = "payload")]`.

| Event Type | Payload Fields | Purpose |
|---|---|---|
| `ConnectionStatus` | `{ status: ConnectionStatus }` | Sync connection lifecycle (Disconnected, Connecting, Syncing, Connected, Reconnecting, Terminated) |
| `AuthState` | `{ account: Account \| null }` | Current session state |
| `RoomListDiff` | `{ subscription_id: SubscriptionId, diff: VectorDiffDto<RoomSummary> }` | Differential updates to active room list |
| `TimelineDiff` | `{ subscription_id: SubscriptionId, room_id: RoomId, diff: VectorDiffDto<TimelineItemDto> }` | Differential updates to virtualized room timeline |
| `TypingNotice` | `{ room_id: RoomId, users: UserId[] }` | Ephemeral typing notifications |
| `VerificationState` | `{ user_id: UserId, device_id: string, state: VerificationState }` | Interactive device verification state machine updates |
| `RecoveryState` | `{ state: RecoveryState }` | Server-side key backup health |
| `ToastNotice` | `{ message: string, level: string }` | Global feedback alerts |

---

## 5. Differential List Synchronization (`VectorDiffDto<T>`)

The UI maintains list state by applying differential operations in-place using `applyDiff(target, diff)`:

```typescript
export function applyDiff<T>(target: T[], diff: VectorDiffDto<T>): T[];
```

All 11 variants map directly to `eyeball_im::VectorDiff`:
1. `Append { values }`: Appends elements to the end.
2. `Clear`: Empties the list.
3. `PushFront { value }`: Prepends element at index 0.
4. `PushBack { value }`: Appends single element at the end.
5. `PopFront`: Removes element at index 0.
6. `PopBack`: Removes element at end.
7. `Insert { index, value }`: Inserts element at index.
8. `Set { index, value }`: Replaces element at index.
9. `Remove { index }`: Removes element at index.
10. `Truncate { length }`: Truncates list to given length.
11. `Reset { values }`: Replaces entire contents with new elements.

---

## 6. Error Handling (`EngineError`)

Errors crossing the boundary are structured and safe:
```rust
pub struct EngineError {
    pub code: String,
    pub message: String,
}
```

Standard error codes:
- `not_implemented`: Command handler not yet wired. Contains command name in message.
- `auth_failed`: Invalid credentials, expired session, or OIDC failure.
- `network_error`: Network timeout or homeserver unreachable.
- `not_found`: Room, event, or account ID does not exist.
- `invalid_argument`: Invalid input format, invalid room alias, or malformed data.
- `internal_error`: Unexpected engine error. Stack traces and raw internal details are scrubbed.

---

## 7. Subscription Lifecycle

1. Frontend initiates subscription via invoke (e.g. `SubscribeRoomList` or `SubscribeTimeline`).
2. Backend generates a unique `SubscriptionId` (e.g. `sub_tl_1234`), starts observing the stream, and returns `{ subscription_id }`.
3. Backend immediately emits an initial `Reset` or `Append` diff to populate current state.
4. Subsequent updates stream as incremental diffs (`PushBack`, `Set`, `Remove`, etc.) with matching `subscription_id`.
5. Frontend cancels when leaving room or unmounting via `Unsubscribe { subscription_id }`.
6. When frontend channel closes (e.g. tab refresh), backend cleans up orphan subscriptions.

---

## 8. Ownership Map for Future Parallel Streams

| Stream | Owned Files & Directories | Responsibilities |
|---|---|---|
| **core-auth** | `crates/alloy-core/src/auth.rs` | OIDC authorization code flow, MAS negotiation, password login fallback, token refresh |
| **core-rooms-timeline** | `crates/alloy-core/src/rooms.rs`, `crates/alloy-core/src/timeline.rs`, `crates/alloy-core/src/mapping.rs` | Sliding sync RoomListService, TimelineBuilder, send queue local echo, read receipts, reactions |
| **core-crypto** | `crates/alloy-core/src/crypto.rs` | Vodozemac Megolm/Olm sessions, device SAS verification, secret storage key backup (`m.megolm_backup.v1`) |
| **core-media** | `crates/alloy-core/src/media.rs` | MSC3916 authenticated media download, encryption/decryption of attachments, disk thumbnail cache |
| **theme-runtime** | `crates/alloy-theme/**`, `packages/themes/**` | Token schema, CSS variable generator, theme loader, hot reload |
| **ui-shell-timeline** | `apps/alloy/ui/src/routes/**`, `apps/alloy/ui/src/lib/components/**` | Adaptive shell, virtualized timeline rendering (`virtua`), composer, HUD |
| **ui-ipc-glue** | `apps/alloy/ui/src/lib/ipc/**` | Transport implementations, reactive runes stores, client helpers |
| **app-router** | `apps/alloy/src-tauri/src/engine.rs`, `apps/alloy/src-tauri/src/commands.rs` | IPC router dispatching to module handlers |

---

## 9. Rules for Evolving Protocol v1

1. **Additive Only**: Within `v0.x`, commands, response variants, event variants, and optional struct fields may only be added. No existing variants or fields may be renamed or removed.
2. **Specta Derivation**: Every DTO must derive `serde::Serialize`, `serde::Deserialize`, and `specta::Type`.
3. **Idempotent Regeneration**: Whenever `crates/alloy-protocol/src/lib.rs` is modified, regenerate TypeScript types using:
   ```bash
   cargo run -p alloy-protocol --bin generate-types
   ```
   Type generation must be clean and idempotent (no git diff after running twice).
4. **Validation Checklist**:
   - `cargo fmt --all`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `pnpm --prefix apps/alloy/ui check`
   - `pnpm --prefix apps/alloy/ui test`
   - `pnpm --prefix apps/alloy/ui build`
