//! Client-side cryptography and verification for Alloy's optional sync server (ADR 0020).
//!
//! The server is a byte store plus one compare-and-swap head. It holds ciphertext under opaque
//! names and is assumed hostile: root on the server, or a malicious operator, can read, rewrite,
//! replay, truncate and fork everything it serves. This crate is the client's half of the trust
//! boundary and contains no network code:
//!
//! - [`keys`] — the master key and the HKDF-derived key hierarchy, with distinct key types.
//! - [`blob`] — the authenticated, padded blob codec. Its AAD binds layer, logical id, seq, epoch
//!   and both header versions, so a replay or a relabel fails to decrypt.
//! - [`chain`] — the signed batch chain and head store. A replayed or forked batch is refused with
//!   a typed error and leaves local state untouched.
//! - [`pin`] — endpoint SPKI pinning as a value type.
//! - [`client_state`] — the versioned, persistent cursor.
//!
//! Nothing here implements a cryptographic primitive: AEAD, HKDF, SHA-256 and Ed25519 come from
//! RustCrypto, and the crate forbids `unsafe` through the workspace lint.

pub mod blob;
pub mod chain;
pub mod client_state;
pub mod keys;
pub mod pin;

pub use blob::{BlobAddr, BlobError, Layer, MAX_BLOB_BYTES, PAD_CLASS, open, seal};
pub use chain::{
    BatchHeader, ChainError, FileHeadStore, Head, HeadRecord, HeadStore, InMemoryHeadStore,
    SignedBatch, verify_batch,
};
pub use client_state::{CLIENT_STATE_VERSION, ClientState, ClientStateError, DeviceCursor};
pub use keys::{AddrKey, BlobKey, BlobKeyRing, MASTER_KEY_LEN, MasterKey, RecoveryKey, RosterKey};
pub use pin::{PIN_LEN, Pin, PinError};
