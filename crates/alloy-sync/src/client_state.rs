//! The client's persistent sync cursor.
//!
//! One JSON object per device (or per account on that device) carries everything the client must
//! remember across restarts: the verified head per device, the epochs whose blob keys it still
//! holds, the pinned endpoint and the last refusal it reported. The file is versioned; a version
//! this build does not know is an error, never a silent migration, because guessing at older or
//! newer fields would weaken the fork detection above it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use crate::chain::Head;
use crate::keys::{BlobKeyRing, MasterKey};
use crate::pin::Pin;

/// The only client-state version this build reads and writes.
pub const CLIENT_STATE_VERSION: u32 = 1;

/// Where one device's chain has reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCursor {
    pub seq: u64,
    pub head: Head,
    pub epoch: u32,
}

/// The cursor a client keeps between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientState {
    pub version: u32,
    /// Device id to that device's last verified head.
    pub devices: BTreeMap<String, DeviceCursor>,
    /// Epochs the client can still decrypt. Rotation adds one and never removes old ones until
    /// the operator asks to forget them.
    pub known_epochs: BTreeSet<u32>,
    /// Pinned endpoint SPKI hash, if an endpoint is configured.
    pub pin: Option<Pin>,
    /// The last typed refusal, rendered for the UI or log. Never used for correctness.
    pub last_error: Option<String>,
}

impl Default for ClientState {
    fn default() -> Self {
        Self::new([0])
    }
}

impl ClientState {
    /// A fresh client state that knows the given epochs.
    pub fn new(known_epochs: impl IntoIterator<Item = u32>) -> Self {
        Self {
            version: CLIENT_STATE_VERSION,
            devices: BTreeMap::new(),
            known_epochs: known_epochs.into_iter().collect(),
            pin: None,
            last_error: None,
        }
    }

    /// The last verified head for `device_id`.
    pub fn cursor(&self, device_id: &str) -> Option<&DeviceCursor> {
        self.devices.get(device_id)
    }

    /// Record a verified head. Only call this after [`crate::chain::verify_batch`] returned it.
    pub fn record_head(&mut self, device_id: impl Into<String>, seq: u64, head: Head, epoch: u32) {
        self.devices
            .insert(device_id.into(), DeviceCursor { seq, head, epoch });
    }

    /// Store the typed violation the client just refused, so the next run and the UI can show it.
    pub fn note_error(&mut self, error: impl fmt::Display) {
        self.last_error = Some(error.to_string());
    }

    /// Clear the last violation after a successful read or an explicit acknowledgement.
    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    /// Blob keys for every epoch this state still knows.
    pub fn blob_keys(&self, master: &MasterKey) -> BlobKeyRing {
        BlobKeyRing::new(master, self.known_epochs.iter().copied())
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ClientStateError> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Parse a stored state. The version is inspected before any other field, so an unknown
    /// version is reported as such instead of as an incidental deserialization failure.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ClientStateError> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or(ClientStateError::MissingVersion)?;
        if version != u64::from(CLIENT_STATE_VERSION) {
            return Err(ClientStateError::UnsupportedVersion {
                found: version,
                supported: CLIENT_STATE_VERSION,
            });
        }
        let wire: ClientStateWire = serde_json::from_value(value)?;
        Ok(Self::from_wire(wire))
    }

    fn from_wire(wire: ClientStateWire) -> Self {
        Self {
            version: wire.version,
            devices: wire.devices,
            known_epochs: wire.known_epochs,
            pin: wire.pin,
            last_error: wire.last_error,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientStateWire {
    version: u32,
    #[serde(default)]
    devices: BTreeMap<String, DeviceCursor>,
    #[serde(default)]
    known_epochs: BTreeSet<u32>,
    #[serde(default)]
    pin: Option<Pin>,
    #[serde(default)]
    last_error: Option<String>,
}

impl<'de> Deserialize<'de> for ClientState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ClientStateWire::deserialize(deserializer)?;
        if wire.version != CLIENT_STATE_VERSION {
            return Err(de::Error::custom(format!(
                "client state version {} is not supported; this build reads and writes {}",
                wire.version, CLIENT_STATE_VERSION
            )));
        }
        Ok(Self::from_wire(wire))
    }
}

/// Client-state parsing errors.
#[derive(Debug, Error)]
pub enum ClientStateError {
    #[error("client state is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("client state has no version field")]
    MissingVersion,
    #[error(
        "client state version {found} is not supported; this build reads and writes {supported}"
    )]
    UnsupportedVersion { found: u64, supported: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::HEAD_LEN;

    fn state() -> ClientState {
        let mut state = ClientState::new([0, 3]);
        state.record_head("device-a", 4, Head::from_bytes([1; HEAD_LEN]), 3);
        state.record_head("device-b", 1, Head::from_bytes([2; HEAD_LEN]), 0);
        state.pin = Some(Pin::of_spki(b"spki"));
        state.note_error("batch branches from a recorded head");
        state
    }

    #[test]
    fn json_round_trip_keeps_every_field() {
        let state = state();
        let json = state.to_json().unwrap();
        assert_eq!(ClientState::from_json(&json).unwrap(), state);
        assert_eq!(serde_json::from_slice::<ClientState>(&json).unwrap(), state);
    }

    #[test]
    fn unknown_version_is_refused_not_migrated() {
        let json = br#"{"version":2,"devices":{},"known_epochs":[]}"#;
        let err = ClientState::from_json(json).unwrap_err();
        assert!(matches!(
            err,
            ClientStateError::UnsupportedVersion {
                found: 2,
                supported: CLIENT_STATE_VERSION
            }
        ));

        let err = serde_json::from_slice::<ClientState>(json).unwrap_err();
        assert!(err.to_string().contains("version 2 is not supported"));
    }

    #[test]
    fn missing_version_is_refused() {
        let err = ClientState::from_json(br#"{"devices":{}}"#).unwrap_err();
        assert!(matches!(err, ClientStateError::MissingVersion));
    }

    #[test]
    fn unknown_fields_are_refused() {
        let err = ClientState::from_json(
            br#"{"version":1,"devices":{},"known_epochs":[0],"surprise":true}"#,
        )
        .unwrap_err();
        assert!(matches!(err, ClientStateError::Json(_)));
    }

    #[test]
    fn blob_keys_follow_the_known_epochs() {
        let state = state();
        let master = MasterKey::from_bytes([5u8; 32]);
        let ring = state.blob_keys(&master);
        assert_eq!(ring.epochs().collect::<Vec<_>>(), vec![0, 3]);
        assert!(!ring.contains(1));
    }

    #[test]
    fn default_state_has_the_current_version() {
        assert_eq!(ClientState::default().version, CLIENT_STATE_VERSION);
    }
}
