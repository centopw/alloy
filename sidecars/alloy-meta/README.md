# alloy-meta

Alloy's Facebook Messenger connector, shipped as a separate executable.

This program is the M3 sidecar described in
[`docs/plan/10-connector-protocol.md`](../../docs/plan/10-connector-protocol.md) and
[ADR 0016](../../docs/decisions/0016-on-device-connectors.md). It logs in to a personal
Messenger account with the account's own email/phone and password (cookie import is the
fallback), keeps the E2EE device state on the device, and speaks newline-delimited JSON over
stdin/stdout. Alloy never links this code: it starts the binary, writes one request object per
line, and reads responses and events back.

The user signs in with the account's email/phone and password; exporting browser cookies is the
fallback. The login itself is mautrix-meta's Messenger Lite flow, which is pure Go — the Bloks
interpreter replays the official app's login screens, so no browser, webview or JavaScript engine
is involved.

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

`begin_login` returns a `credentials` step:

```json
{"type":"credentials","username_label":"Email or phone number","alternatives":["cookies"]}
```

It asks for the account's email/phone and password, answered by mautrix-meta's Messenger Lite
login. That flow is the CAA login of the official Android app, replayed by mautrix-meta's pure-Go
Bloks interpreter — no browser, no webview, no JavaScript engine. The sidecar drives
`messagix.MessengerLite.DoLoginSteps`, which fetches the lightspeed key (`FetchLightspeedKey`),
encrypts the password inside the interpreter and exchanges the resulting access token for session
cookies (`GetSessionForApp`).

Steps the flow can ask for after the credentials:

| Bloks step | Wire step |
|---|---|
| email/password form | `credentials` |
| password re-entry | `password` |
| 2FA, SMS, WhatsApp, backup or email code | `code` with `via` = `app`, `sms`, `other` or `email` |
| approve-the-login-on-another-device | `code` with `via` = `app` (whatever code is sent is unused) |
| captcha | `failed` with `account_locked` |

Every answer goes back with `submit_login` (`{"type":"code","code":"…"}`,
`{"type":"password","password":"…"}`) and the loop ends with a `done` step. A step that comes back
after it was answered means the network rejected the answer: the connector reports `invalid_input`
on field `code` or `password` with the network's own reason instead of prompting again.
Rate limits produce `rate_limited`, checkpoints and suspicious-activity blocks produce
`account_locked`, and a two-factor method picker or Meta's Google sign-in (which the connector
cannot render) ends with `not_supported`.

Cookie import stays available as the fallback: `submit_login` with
`{"type":"switch","to":"cookies"}` returns the `cookies` step asking for `datr`, `c_user`, `xs`
(required) and `sb`, `fr` (optional). Either path validates the session by loading the inbox; the
resulting `done` step carries an opaque `session` blob that the host must persist before calling
`start`. The blob is base64 JSON — version 2 carries the platform plus the cookies and, when the
Lite flow produced one, the native session (access token and device ids), so a restart restores
the account without a browser or another password round. Version 1 blobs are still restored.
Cookies that Facebook rejects produce `invalid_input` on field `cookies`.

## E2EE

Messenger 1:1 chats are end-to-end encrypted. The sidecar runs mautrix-meta's messagix E2EE
client (whatsmeow under the hood) and stores its device/session database at
`$ALLOY_SIDECAR_DATA_DIR/<remote_user_id>/e2ee.db`. If `ALLOY_SIDECAR_DATA_DIR` is unset it
falls back to the platform user config directory. The SQLite driver is pure Go
(`modernc.org/sqlite`), so the binary cross-compiles without cgo.

## Account-lock risk

Automating a personal Facebook account is against Meta's terms and can lead to a checkpoint or
an account restriction, especially when a session is used from a new address. This is a known
risk of the messagix approach, documented upstream:
<https://github.com/mautrix/meta/issues/44>. Logging in with a password is no riskier than
importing cookies — both produce the same kind of session — but the network may answer either
with a checkpoint, and only the user can clear that on Facebook's own site or in the official
app. The sidecar never creates accounts and never bypasses a checkpoint; it reports
`account_locked` and stops.

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
