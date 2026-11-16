//! Endpoint pinning as a value type (ADR 0020 invariant 12).
//!
//! The enrolment payload carries the sync endpoint's SPKI hash. A changed pin blocks sync and
//! warns; nothing here talks to TLS or the network, so the crate stays testable without a
//! certificate stack. The pin is the SHA-256 of the endpoint certificate's SubjectPublicKeyInfo
//! DER, stored as 64 lower-case hex characters.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Length of an SPKI SHA-256 pin.
pub const PIN_LEN: usize = 32;

/// One pinned endpoint identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pin {
    spki_sha256: [u8; PIN_LEN],
}

impl Pin {
    /// Pin the SPKI DER bytes of a certificate. Trust-on-first-use callers use this once; after
    /// that they compare with [`Pin::verify_spki`].
    pub fn of_spki(spki_der: &[u8]) -> Self {
        let digest = Sha256::digest(spki_der);
        let mut bytes = [0u8; PIN_LEN];
        bytes.copy_from_slice(&digest);
        Self { spki_sha256: bytes }
    }

    /// Parse the exact 64-character hex form. Whitespace and prefixes are rejected, so a pin is
    /// either the value that was pinned or an error.
    pub fn parse(text: &str) -> Result<Self, PinError> {
        if text.len() != PIN_LEN * 2 {
            return Err(PinError::Malformed {
                reason: "pin must be 64 hex characters",
            });
        }
        let mut bytes = [0u8; PIN_LEN];
        for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let pair = std::str::from_utf8(pair).map_err(|_| PinError::Malformed {
                reason: "pin is not ASCII hex",
            })?;
            bytes[index] = u8::from_str_radix(pair, 16).map_err(|_| PinError::Malformed {
                reason: "pin is not hex",
            })?;
        }
        Ok(Self { spki_sha256: bytes })
    }

    /// The canonical lower-case hex form.
    pub fn to_hex(self) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(PIN_LEN * 2);
        for byte in self.spki_sha256 {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    pub fn as_bytes(&self) -> &[u8; PIN_LEN] {
        &self.spki_sha256
    }

    /// Compare the pin against the SPKI DER the endpoint actually presented.
    pub fn verify_spki(&self, spki_der: &[u8]) -> Result<(), PinError> {
        let found = Self::of_spki(spki_der);
        if found == *self {
            Ok(())
        } else {
            Err(PinError::Mismatch {
                expected: self.to_hex(),
                found: found.to_hex(),
            })
        }
    }
}

impl fmt::Display for Pin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Pin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Pin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(PinVisitor)
    }
}

struct PinVisitor;

impl Visitor<'_> for PinVisitor {
    type Value = Pin;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 64-character hex SPKI pin")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Pin::parse(value).map_err(E::custom)
    }
}

/// Pin parsing and comparison errors. A mismatch is not a parse error: the endpoint answered, but
/// it is not the pinned endpoint.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PinError {
    #[error("pin is malformed: {reason}")]
    Malformed { reason: &'static str },
    #[error("endpoint SPKI pin changed: expected {expected}, got {found}")]
    Mismatch { expected: String, found: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPKI: &[u8] = b"not a real certificate, but stable test bytes";

    #[test]
    fn parse_and_format_round_trip() {
        let pin = Pin::of_spki(SPKI);
        assert_eq!(Pin::parse(&pin.to_hex()).unwrap(), pin);
        assert_eq!(pin.to_string().len(), PIN_LEN * 2);
    }

    #[test]
    fn matching_spki_verifies() {
        let pin = Pin::of_spki(SPKI);
        assert_eq!(pin.verify_spki(SPKI), Ok(()));
    }

    #[test]
    fn changed_spki_is_a_typed_mismatch() {
        let pin = Pin::of_spki(SPKI);
        let err = pin.verify_spki(b"a different certificate").unwrap_err();
        assert!(matches!(err, PinError::Mismatch { .. }));
    }

    #[test]
    fn malformed_pins_are_typed() {
        let not_hex = "g".repeat(64);
        for text in ["", "abcd", not_hex.as_str()] {
            assert!(matches!(Pin::parse(text), Err(PinError::Malformed { .. })));
        }
    }

    #[test]
    fn serde_uses_the_hex_form() {
        let pin = Pin::of_spki(SPKI);
        let json = serde_json::to_string(&pin).unwrap();
        assert_eq!(json, format!("\"{}\"", pin.to_hex()));
        assert_eq!(serde_json::from_str::<Pin>(&json).unwrap(), pin);
    }
}
