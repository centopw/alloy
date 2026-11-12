//! The serializable connector contract.
//!
//! Alloy connects a user's personal Telegram and Messenger accounts directly from the device
//! (ADR 0016). A **connector** implements one network behind [`Connector`]; the engine owns one
//! actor per account, an outbox, and the local store. This crate is the seam: it contains no
//! network code, no database code and no engine code, so the same contract can be driven
//! in-process, across a sidecar process, or by the optional M6 gateway.
//!
//! - [`types`] — networks, chats, users, messages, media, session blobs.
//! - [`capabilities`] — per-network feature flags and limits.
//! - [`login`] — the interactive login state machine the UI renders.
//! - [`connector`] — the [`Connector`] trait and its request/result values.
//! - [`events`] — live updates pushed after `start`.
//! - `rpc` — newline-delimited JSON framing for out-of-process connectors.
//! - `fake` (feature `testing`) — deterministic in-memory connector for contract and E2E tests.
//!
//! The design is specified in `docs/plan/10-connector-protocol.md`.
//!
//! ```
//! use alloy_connector::{Network, SessionBlob};
//!
//! assert_eq!(serde_json::to_string(&Network::Messenger).unwrap(), "\"messenger\"");
//! assert_eq!(serde_json::to_string(&SessionBlob::new(vec![0, 1, 2, 255])).unwrap(), "\"AAEC/w==\"");
//! ```

pub mod capabilities;
pub mod connector;
pub mod error;
pub mod events;
pub mod login;
pub mod rpc;
pub mod types;

#[cfg(feature = "testing")]
pub mod fake;

pub use capabilities::Capabilities;
pub use connector::{Connector, LocalAttachment, OutgoingMessage, Page, SendAck};
pub use error::ConnectorError;
pub use events::{ConnectionState, ConnectorEvent};
pub use login::{CodeChannel, CookieField, LoginFailure, LoginInput, LoginStep};
pub use rpc::{Frame, PROTOCOL_VERSION, decode_frame, encode_frame};
pub use types::{
    ChatKind, MediaRef, MessageKind, Network, RemoteChat, RemoteMessage, RemoteUser, SessionBlob,
};
