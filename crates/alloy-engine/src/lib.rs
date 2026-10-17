#![forbid(unsafe_code)]
//! # alloy-engine
//!
//! Core engine: `Engine` trait, event fan-out, subscription registry, router dispatch to group handlers.
//!
//! Decoupled from Tauri so it can be unit-tested standalone.

pub mod context;
pub mod handlers;
pub mod registry;
pub mod router;
pub mod sink;

pub mod error {
    pub use alloy_protocol::EngineError;
}

pub use alloy_protocol::EngineError;
pub use context::EngineContext;
pub use registry::SubscriptionRegistry;
pub use router::{AppEngine, Engine, GroupHandler, RouterEngine};
pub use sink::EventSink;
