# alloy-meta

Alloy's Facebook Messenger connector, shipped as a separate executable.

This program is the M3 sidecar described in
[`docs/plan/10-connector-protocol.md`](../../docs/plan/10-connector-protocol.md) and
[ADR 0016](../../docs/decisions/0016-on-device-connectors.md). It logs in to a personal
messenger.com account with the user's own browser cookies, keeps the E2EE device state on the
device, and speaks newline-delimited JSON over stdin/stdout. Alloy never links this code: it
starts the binary, writes one request object per line, and reads responses and events back.

## Protocol

- One compact JSON object per line, no pretty printing, on stdin and stdout. Diagnostics go to
  stderr only.
- The sidecar writes `{"type":"hello","protocol_version":1,"network":"messenger","capabilities":…}`
  before it accepts any other frame. A host that sees a different `protocol_version` must refuse
  the sidecar and terminate it.
- Requests are `{"type":"request","id":N,"method":"…","params":{…}}`; every request gets exactly
  one `{"type":"response","id":N,"result":…,"error":…}` with exactly one of the two set.
- Events are `{"type":"event","event":{"type":"…"}}` and may be interleaved with responses.
- Method names are the `Connector` trait methods in snake_case: `begin_login`, `submit_login`,
  `restore`, `start`, `stop`, `list_chats`, `backfill`, `send`, `edit`, `delete`, `react`,
  `unreact`, `mark_read`, `set_typing`, `fetch_media`, `logout`.
- Remote ids are opaque strings. Messenger threads use their numeric thread key; end-to-end
  encrypted chats use `e2ee:<jid>`, so the two id spaces never collide.

## Login

`begin_login` returns a `cookies` step asking for `datr`, `c_user`, `xs` (required) and `sb`,
`fr` (optional). `submit_login` validates them by loading the inbox; the resulting `done` step
carries an opaque `session` blob (base64 JSON with the cookie set) that the host must persist
before calling `start`. Cookies that Facebook rejects produce `invalid_input` on field `cookies`;
a checkpoint or suspicious-activity block produces `account_locked`.

## E2EE

Messenger 1:1 chats are end-to-end encrypted. The sidecar runs mautrix-meta's messagix E2EE
client (whatsmeow under the hood) and stores its device/session database at
`$ALLOY_SIDECAR_DATA_DIR/<remote_user_id>/e2ee.db`. If `ALLOY_SIDECAR_DATA_DIR` is unset it
falls back to the platform user config directory. The SQLite driver is pure Go
(`modernc.org/sqlite`), so the binary cross-compiles without cgo.

## Account-lock risk

Automating a personal Facebook account is against Meta's terms and can lead to a checkpoint or
an account restriction, especially when the cookie session is used from a new address. This is a
known risk of the messagix approach, documented upstream:
<https://github.com/mautrix/meta/issues/44>. The sidecar never creates accounts and never
bypasses a checkpoint; the user resolves those on Facebook's own site.

## Licence

AGPL-3.0-or-later. The sidecar reuses `go.mau.fi/mautrix-meta/messagix` (AGPL-3.0), so the whole
binary is AGPL-3.0-or-later; see `LICENSE`. Shipping it alongside Alloy (Apache-2.0) is fine
because it is a separate executable that communicates only over stdio.

## Build

```sh
cd sidecars/alloy-meta
go build -o alloy-meta .
```

Run it from Alloy's connector supervisor; running it by hand is only useful for protocol
debugging:

```sh
echo '{"type":"request","id":1,"method":"capabilities","params":{}}' | ./alloy-meta
```
