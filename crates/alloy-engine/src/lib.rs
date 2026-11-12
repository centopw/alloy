#![recursion_limit = "256"]
#![forbid(unsafe_code)]
//! # alloy-engine
//!
//! The framework-agnostic command layer of the on-device client (ADR 0016).
//!
//! One tokio actor per connected account drives an [`alloy_connector::Connector`], persists every
//! remote change in `alloy-store`, and a per-account outbox worker pushes local operations through
//! the same connector. The UI receives derived projections (room list, timelines) as
//! [`alloy_protocol::VectorDiffDto`]s, computed from the store and refreshed through
//! [`projection::ChangeBus`], never by polling the database.
//!
//! The [`Engine`](router::Engine) trait, the group routers and the event sink stay decoupled from
//! Tauri so handlers can be tested standalone.

pub mod accounts;
pub mod actor;
pub mod context;
pub mod convert;
pub mod error;
pub mod factory;
pub mod files;
pub mod handlers;
pub mod login;
pub mod outbox;
pub mod projection;
pub mod registry;
pub mod router;
pub mod sink;
pub mod support;

pub use context::EngineContext;
pub use error::EngineError;
pub use registry::SubscriptionRegistry;
pub use router::{AppEngine, CommandGroup, Engine, GroupHandler, RouterEngine};
pub use sink::EventSink;
