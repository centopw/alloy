//! Newline-delimited JSON framing for connectors that run in a separate process.
//!
//! The Messenger sidecar (M3) speaks this protocol on stdin/stdout; the M6 gateway reuses the same
//! frames over WSS. One JSON object per line, one line per write, no pretty-printing:
//!
//! ```text
//! {"type":"hello","protocol_version":1,"network":"messenger","capabilities":{...}}
//! {"type":"request","id":7,"method":"list_chats","params":{"cursor":null,"limit":50}}
//! {"type":"response","id":7,"result":{"items":[],"next_cursor":null},"error":null}
//! {"type":"event","event":{"type":"connection","state":{"type":"online"}}}
//! ```
//!
//! The sidecar sends [`Frame::Hello`] first and nothing else until the host has accepted the
//! version. Requests are correlated by `id`, which the host chooses and the sidecar only echoes;
//! events may be interleaved between any two frames. Method names are the
//! [`Connector`](crate::Connector) method names in snake_case, and `params` matches the method's
//! arguments as a JSON object.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::capabilities::Capabilities;
use crate::error::ConnectorError;
use crate::events::ConnectorEvent;
use crate::types::Network;

/// Version of this framing and of the request/event shapes carried inside it.
///
/// A host MUST refuse a sidecar whose `Hello` announces any other version: a mismatch means the
/// two ends disagree about the meaning of a method or a frame.
pub const PROTOCOL_VERSION: u32 = 1;

/// One NDJSON line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
// The event frame carries a whole `ConnectorEvent`; boxing it would only add an allocation to
// the hot path that the sidecar already pays for on the wire.
#[allow(clippy::large_enum_variant)]
pub enum Frame {
    /// First frame from the sidecar: identity, protocol version and capabilities.
    Hello {
        protocol_version: u32,
        network: Network,
        capabilities: Capabilities,
    },
    /// Host to sidecar: invoke a connector method.
    Request {
        /// Host-chosen correlation id, unique for the life of the process.
        id: u64,
        /// Connector method name in snake_case, e.g. `"backfill"`.
        method: String,
        /// Method arguments as a JSON object keyed by argument name.
        params: Value,
    },
    /// Sidecar to host: the outcome of one request. Exactly one of `result`/`error` is set.
    Response {
        /// Echo of [`Frame::Request`]'s `id`.
        id: u64,
        /// JSON representation of the method's return value; `null` for unit-returning methods.
        result: Option<Value>,
        /// Failure. Present means `result` is `None`.
        error: Option<ConnectorError>,
    },
    /// Sidecar to host: a live update, independent of any request.
    Event { event: ConnectorEvent },
}

/// Serializes one frame as a single JSON line terminated by `\n`.
///
/// Serialization cannot fail for these types: they contain no maps with non-string keys and no
/// floats.
pub fn encode_frame(frame: &Frame) -> String {
    let mut line = serde_json::to_string(frame).expect("connector frames always serialize");
    line.push('\n');
    line
}

/// Parses one line (with or without its trailing `\n`) into a frame.
///
/// Empty input, unknown shapes and unknown error variants are reported as
/// [`ConnectorError::Protocol`], because they mean the peer is not speaking this protocol.
pub fn decode_frame(line: &str) -> Result<Frame, ConnectorError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ConnectorError::Protocol {
            message: "empty frame".to_string(),
        });
    }

    serde_json::from_str(trimmed).map_err(|error| ConnectorError::Protocol {
        message: format!("invalid frame: {error}"),
    })
}
