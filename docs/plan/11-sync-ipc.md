# Sync IPC contract (ADR 0020)

Surface between the engine and the UI for device pairing, the configuration bundle and the optional
sync server. The engine owns every secret and every byte on the wire; the UI never sees a key, a
token, or a ciphertext blob.

Conventions follow `06-ipc-contract.md`: `EngineCommand` in, `EngineResponse` out, `EngineEvent` for
anything the user did not ask for. Errors are `EngineError` variants; the UI renders `message` and
never parses it.

## Commands

### Configuration bundle

| Command | Response | Notes |
|---|---|---|
| `ExportConfigBundle { include_backgrounds, include_themes }` | `ConfigBundleExported { bundle_json, bytes, summary }` | `summary` names what is inside (counts per layer) so the UI can show it before the user shares the string. Backgrounds are large; the flag is off by default in the UI. |
| `ImportConfigBundle { bundle_json, apply }` | `ConfigBundleImported { report, applied }` | `apply: false` is a dry run that returns the same `report` without writing. A schema mismatch fails with `EngineError::BundleRejected`, naming both versions. |

### Pairing (LAN, no server)

| Command | Response |
|---|---|
| `BeginPairing { role }` where `role` is `"offer"` or `"accept"` | `PairingStarted { pairing_id, ticket, code, endpoint, expires_at_ms }` |
| `AcceptPairingTicket { ticket, code }` | `PairingStarted { … }` on the accepting side |
| `PairingStatus { pairing_id }` | `PairingState { state, sas, peer_label, transferred, error }` |
| `ConfirmPairing { pairing_id, accept }` | `PairingState` |

`ticket` is the QR payload string from `alloy_storage::pairing`; the UI renders it as a QR code and
shows `code` beside it, because a device without a camera types the code. `state` is one of
`waiting | peer_found | sas_ready | transferring | done | declined | expired | failed`.
The bundle crosses only after `ConfirmPairing { accept: true }` on both sides.

### Sync server

| Command | Response |
|---|---|
| `ConfigureSync { url, token, mode }` | `SyncConfigured { pin, status }` |
| `SyncStatus` | `SyncStatusReport { mode, url, pin, state, last_sync_ms, last_error, layers, devices }` |
| `SyncNow` | `SyncCompleted { uploaded, downloaded, head_hex, epoch }` |
| `ListDevices` | `DevicesList { devices }` |
| `RevokeDevice { device_id }` | `DeviceRevoked { epoch }` |

`mode` is `"off" | "prefs"`. `ConfigureSync` performs a `GET /v1/head` against `url` with the token,
derives the pin from the TLS leaf's SPKI, and returns it; a pin change on a later run fails with
`EngineError::PinMismatch { stored, presented }` unless the caller passes the same URL again with
`accept_new_pin: true`. Tokens and the master key live in the secret store (ADR 0019), never in
`settings.json`.

`layers` is `[{ key, enabled, updated_at_ms }]` with `key` in
`settings | tags | catalogue | backgrounds | theme | messages`. `messages` is the opt-in mirror
(slice F) and is reported as `enabled: false` until that slice ships.

### Recovery

| Command | Response |
|---|---|
| `CreateRecoveryPhrase` | `RecoveryPhraseCreated { words }` |
| `VerifyRecoveryPhrase { words }` | `RecoveryVerified` |
| `RecoverFromPhrase { words, url, token, accept_new_pin }` | `Recovered { layers, devices }` |

A fresh device has no pin yet, so the first https connection captures it unless `accept_new_pin`
is false; loopback `http` is the only unpinned transport. The master key is stored only after the
phrase opens the escrow **and** the verified head and layers arrive, so a failure leaves a
retryable device rather than a half-adopted account.

The phrase exists only in this response; it is never stored, and the engine keeps only the wrapped
escrow blob it produces.

## Events

| Event | When |
|---|---|
| `SyncStateChanged { state, detail }` | sync idle → running → done/failed, and on a pin mismatch or a head-chain violation, so the UI can surface it without polling |
| `PairingChanged { pairing_id, state, sas }` | any transition of a live pairing, including expiry |

A head-chain violation (`Fork`, `Branch`, `Signature`) is a `SyncStateChanged { state: "blocked" }`
with a reason code — never a silent retry, and never a "done".

## UI

One section, `Devices & Sync` (Settings → Security), with:

1. **This device** — label, enrolled at, epoch.
2. **Pair a device** — Offer: QR + 8-character code + SAS confirmation. Accept: paste ticket or type
   code, then SAS confirmation. Both paths say what crosses ("settings, tags, groups, theme") before
   the transfer starts.
3. **Server** — mode off/prefs, URL, token field (write-only: shows "stored", never the value), pin
   fingerprint, test connection, last sync, sync now.
4. **Devices** — table with label, last sync, revoke. Revoking states that it rotates the key epoch
   and that past data on that device cannot be recalled.
5. **Recovery phrase** — create, verify, and the plain warning that without it a new device needs a
   surviving enrolled device.

Nothing in this section may block the rest of settings while a pairing or a sync runs.
