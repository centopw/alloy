//! Telegram personal-account connector on [grammers].
//!
//! Implements [`alloy_connector::Connector`] for one Telegram account: it speaks MTProto
//! directly from the device (ADR 0016), stores nothing, and returns every credential as a
//! [`alloy_connector::SessionBlob`] the host encrypts and keeps.
//!
//! [grammers]: https://codeberg.org/Lonami/grammers
//!
//! # Session blobs
//!
//! A blob is JSON with a version envelope:
//!
//! ```json
//! { "v": 1, "data": { "home_dc": 2, "dc_options": [ … ], "peer_infos": [ … ], "updates_state": { … } },
//!   "peers": { "u123": { "bare_id": 123, "kind": "user", "auth": -456 } },
//!   "myself": { "remote_user_id": "u123", "bare_id": 123, "display_name": "Ada" } }
//! ```
//!
//! `data` is grammers' session state (datacenter addresses and authorization keys, the peer
//! cache grammers needs to resolve peers, and the update-state checkpoint). `peers` is this
//! connector's own id → access-hash cache, so [`Connector::restore`] can resolve chat ids it
//! was given earlier without listing dialogs again. Any other version, or bytes that are not
//! this shape, are [`alloy_connector::ConnectorError::Unauthorized`]: only a fresh login helps.
//!
//! [`Connector::restore`]: alloy_connector::Connector::restore
//!
//! The host must persist the blob it receives in [`LoginStep::Done`] before calling
//! [`Connector::start`], and replace its copy whenever [`ConnectorEvent::SessionUpdated`] arrives
//! (at most once every 60 s while updates flow, so an update-state gap after a crash stays small).
//!
//! [`Connector::start`]: alloy_connector::Connector::start
//! [`SessionBlob`]: alloy_connector::SessionBlob
//! [`LoginStep::Done`]: alloy_connector::LoginStep::Done
//! [`ConnectorEvent::SessionUpdated`]: alloy_connector::ConnectorEvent::SessionUpdated
//!
//! # Login
//!
//! [`Connector::begin_login`] always returns [`LoginStep::PhoneNumber`]. Two paths lead out of it:
//!
//! - **Phone** — submit [`LoginInput::PhoneNumber`]; the connector calls `auth.sendCode` and
//!   returns [`LoginStep::Code`] with the delivery channel Telegram reports (`app`, `sms`, `email`)
//!   and the expected length when it states one. Submit [`LoginInput::Code`]; Telegram either
//!   accepts the code (a `2fa` account then asks for [`LoginStep::Password`]) or rejects it, which
//!   is a terminal [`LoginStep::Failed`].
//! - **QR** — submit [`LoginInput::QrRefresh`] instead of a phone number (or at any later point of
//!   a QR attempt). The connector calls `auth.exportLoginToken` and returns
//!   [`LoginStep::Qr`] with `payload = "tg://login?token=<base64url>"` and `expires_at`. The user
//!   scans it with an official Telegram app; this connector cannot learn that from the wire while
//!   it is not streaming updates, so the **host contract** is:
//!
//!   1. Render `payload` as a QR code until `expires_at`.
//!   2. At or after `expires_at`, submit [`LoginInput::QrRefresh`] again. The connector always
//!      re-exports the token (that call is where `auth.loginTokenMigrateTo` and
//!      `auth.loginTokenSuccess` are handled, including the datacenter migration Telegram asks
//!      for) and returns a fresh [`LoginStep::Qr`], or [`LoginStep::Done`] once the code was
//!      accepted by the phone.
//!   3. After a scan by an official app the very next `QrRefresh` usually already returns
//!      [`LoginStep::Done`]; a host that only refreshes on expiry delays the login by up to the
//!      token lifetime. Refreshing every few seconds is allowed and cheap.
//!
//!   This connector never polls on its own: `submit_login` is the only place that talks to the
//!   network during login, so a host that stops calling it stops the whole flow.
//!
//! A `Failed` step is terminal; restart with [`Connector::begin_login`]. Sign-up is not supported
//! (`auth.signIn` answering "sign up required" fails the attempt with the `phone` field named):
//! accounts are created in an official Telegram app.
//!
//! [`Connector::begin_login`]: alloy_connector::Connector::begin_login
//! [`LoginInput::PhoneNumber`]: alloy_connector::LoginInput::PhoneNumber
//! [`LoginInput::Code`]: alloy_connector::LoginInput::Code
//! [`LoginInput::QrRefresh`]: alloy_connector::LoginInput::QrRefresh
//! [`LoginStep::PhoneNumber`]: alloy_connector::LoginStep::PhoneNumber
//! [`LoginStep::Code`]: alloy_connector::LoginStep::Code
//! [`LoginStep::Password`]: alloy_connector::LoginStep::Password
//! [`LoginStep::Qr`]: alloy_connector::LoginStep::Qr
//! [`LoginStep::Failed`]: alloy_connector::LoginStep::Failed
//!
//! # Read receipts and typing
//!
//! Telegram's API terms (§1.4) forbid clients that suppress read receipts or typing indicators.
//! This connector always reports both: [`Connector::mark_read`] calls
//! `messages.readHistory`/`channels.readHistory` and [`Connector::set_typing`] calls
//! `messages.setTyping` with the real action. There is no "ghost mode" option and none will be
//! added.
//!
//! [`Connector::mark_read`]: alloy_connector::Connector::mark_read
//! [`Connector::set_typing`]: alloy_connector::Connector::set_typing
//!
//! # Known limits
//!
//! - Reactions arrive as aggregate counts; Telegram only exposes the last few reactors per
//!   message, so [`ConnectorEvent::Reaction`] is emitted for those with `added: true` and removals
//!   are not reported as events (a backfill of the message shows the current totals).
//! - Read receipts from other participants in groups and channels carry no reader identity, so
//!   [`ConnectorEvent::ReadMarker`] is emitted only for direct chats (and for this account's own
//!   position on other devices).
//! - [`Capabilities::location`] and `polls` are `false`: sending them is `NotSupported`. Receiving
//!   a location message still maps to [`alloy_connector::MessageKind::Location`].
//!
//! [`ConnectorEvent::Reaction`]: alloy_connector::ConnectorEvent::Reaction
//! [`ConnectorEvent::ReadMarker`]: alloy_connector::ConnectorEvent::ReadMarker
//! [`Capabilities::location`]: alloy_connector::Capabilities::location

mod config;
mod connector;
mod error;
mod login;
mod mapping;
mod session;
mod updates;

pub use config::{ConfigError, TelegramConfig};
pub use connector::TelegramConnector;
