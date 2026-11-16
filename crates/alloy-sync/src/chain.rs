//! The signed per-device history chain (ADR 0020 invariants 4, 11 and 13).
//!
//! Every uploaded batch carries a [`BatchHeader`] signed with the device's Ed25519 key over a
//! canonical byte encoding: fixed field order, length-prefixed device id, big-endian integers, no
//! JSON. The server enforces one compare-and-swap head, but it is the *client* that decides
//! whether the batch it received belongs to the chain it already trusts:
//!
//! - the signature must verify against the device key;
//! - `seq` must be exactly the successor of the stored head;
//! - `prev_head` must equal the stored head, or name an ancestor the client already recorded.
//!
//! A batch whose parent is a recorded ancestor but not the current head is a [`ChainError::Branch`]:
//! the client can tell a rebase attempt from an ordinary append and from a parent it has never seen
//! ([`ChainError::Fork`]). Failed verification never mutates the [`HeadStore`], so the client keeps
//! its local state and reports the specific violation.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Chain encoding version.
pub const CHAIN_VERSION: u8 = 1;

/// Length of a head hash.
pub const HEAD_LEN: usize = 32;

/// Largest accepted device id. Bounds a server-supplied length before allocation.
pub const MAX_DEVICE_ID: usize = 128;

/// Largest accepted number of recorded heads. Refuses an unbounded ancestry request.
pub const MAX_CHAIN_RECORDS: usize = 4096;

const HEAD_DOMAIN: &[u8] = b"alloy-sync-v1/head";
const BATCH_DOMAIN: &[u8] = b"alloy-sync-v1/batch";
const MAX_BATCH_BYTES: usize = 1 + 2 + MAX_DEVICE_ID + 8 + HEAD_LEN + HEAD_LEN + 4 + 8 + 64;

/// A chain head: SHA-256 over the domain, the canonical header and its signature.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Head([u8; HEAD_LEN]);

impl Head {
    /// The parent of the first batch.
    pub const GENESIS: Head = Head([0u8; HEAD_LEN]);

    pub fn from_bytes(bytes: [u8; HEAD_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; HEAD_LEN] {
        &self.0
    }

    /// Lower-case hex, the form stored in [`HeadRecord`] JSON.
    pub fn to_hex(self) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(HEAD_LEN * 2);
        for byte in self.0 {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    /// Parse the exact 64-character lower- or upper-case hex form.
    pub fn parse_hex(text: &str) -> Result<Self, ChainError> {
        if text.len() != HEAD_LEN * 2 {
            return Err(ChainError::Malformed {
                reason: "head hex must be 64 characters",
            });
        }
        let mut bytes = [0u8; HEAD_LEN];
        for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let pair = std::str::from_utf8(pair).map_err(|_| ChainError::Malformed {
                reason: "head is not ASCII hex",
            })?;
            bytes[index] = u8::from_str_radix(pair, 16).map_err(|_| ChainError::Malformed {
                reason: "head is not hex",
            })?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Debug for Head {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Head({})", self.to_hex())
    }
}

impl Serialize for Head {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Head {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(HeadVisitor)
    }
}

struct HeadVisitor;

impl Visitor<'_> for HeadVisitor {
    type Value = Head;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 64-character hex head")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Head::parse_hex(value).map_err(E::custom)
    }
}

/// The signed metadata of one uploaded batch. Field order is the canonical encoding order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchHeader {
    pub device_id: String,
    pub seq: u64,
    pub prev_head: Head,
    pub root_hash: [u8; HEAD_LEN],
    pub epoch: u32,
    pub created_ms: u64,
}

impl BatchHeader {
    /// Canonical bytes that the signature covers: domain, version, length-prefixed device id, then
    /// integer fields in declaration order. Never `serde_json`, so map ordering cannot vary.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ChainError> {
        if self.device_id.len() > MAX_DEVICE_ID {
            return Err(ChainError::InvalidDeviceId {
                size: self.device_id.len(),
                limit: MAX_DEVICE_ID,
            });
        }
        let mut out = Vec::with_capacity(
            BATCH_DOMAIN.len() + 1 + 2 + self.device_id.len() + 8 + HEAD_LEN + HEAD_LEN + 4 + 8,
        );
        out.extend_from_slice(BATCH_DOMAIN);
        out.push(CHAIN_VERSION);
        out.extend_from_slice(&(self.device_id.len() as u16).to_be_bytes());
        out.extend_from_slice(self.device_id.as_bytes());
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(self.prev_head.as_bytes());
        out.extend_from_slice(&self.root_hash);
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.created_ms.to_be_bytes());
        Ok(out)
    }
}

/// A batch plus its device signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedBatch {
    pub header: BatchHeader,
    pub signature: [u8; 64],
}

impl SignedBatch {
    pub fn sign(header: BatchHeader, key: &SigningKey) -> Result<Self, ChainError> {
        let canonical = header.canonical_bytes()?;
        let signature = key.sign(&canonical).to_bytes();
        Ok(Self { header, signature })
    }

    pub fn verify_signature(&self, key: &VerifyingKey) -> Result<(), ChainError> {
        let canonical = self.header.canonical_bytes()?;
        key.verify_strict(&canonical, &Signature::from_bytes(&self.signature))
            .map_err(|_| ChainError::Signature)
    }

    /// The head this batch produces. Requires a valid signature to have been checked first;
    /// the hash covers both the canonical header and the signature bytes.
    pub fn head(&self) -> Result<Head, ChainError> {
        let canonical = self.header.canonical_bytes()?;
        let mut hasher = Sha256::new();
        hasher.update(HEAD_DOMAIN);
        hasher.update(&canonical);
        hasher.update(self.signature);
        Ok(Head(hasher.finalize().into()))
    }

    /// Transport encoding: version byte, header fields in canonical order, then the signature.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ChainError> {
        let canonical = self.header.canonical_bytes()?;
        // Drop the signature domain and the canonical version byte; the transport frame carries
        // the version once, first.
        let body = &canonical[BATCH_DOMAIN.len() + 1..];
        let mut out = Vec::with_capacity(1 + body.len() + 64);
        out.push(CHAIN_VERSION);
        out.extend_from_slice(body);
        out.extend_from_slice(&self.signature);
        Ok(out)
    }

    /// Parse untrusted transport bytes. Bounded before every read, and trailing bytes are refused
    /// so a server cannot smuggle a second encoding past the client.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ChainError> {
        if bytes.len() > MAX_BATCH_BYTES {
            return Err(ChainError::Malformed {
                reason: "batch over the encoded size limit",
            });
        }
        if bytes.len() < 3 {
            return Err(ChainError::Truncated {
                needed: 3,
                found: bytes.len(),
            });
        }
        if bytes[0] != CHAIN_VERSION {
            return Err(ChainError::UnsupportedVersion {
                found: bytes[0],
                expected: CHAIN_VERSION,
            });
        }
        let device_len = usize::from(u16::from_be_bytes([bytes[1], bytes[2]]));
        if device_len > MAX_DEVICE_ID {
            return Err(ChainError::InvalidDeviceId {
                size: device_len,
                limit: MAX_DEVICE_ID,
            });
        }
        let needed = 3 + device_len + 8 + HEAD_LEN + HEAD_LEN + 4 + 8 + 64;
        if bytes.len() < needed {
            return Err(ChainError::Truncated {
                needed,
                found: bytes.len(),
            });
        }
        if bytes.len() > needed {
            return Err(ChainError::TrailingData {
                extra: bytes.len() - needed,
            });
        }

        let mut cursor = 3;
        let device_id = std::str::from_utf8(&bytes[cursor..cursor + device_len])
            .map_err(|_| ChainError::Malformed {
                reason: "device id is not UTF-8",
            })?
            .to_owned();
        cursor += device_len;
        let seq = take_u64(bytes, &mut cursor);
        let mut prev_head = [0u8; HEAD_LEN];
        prev_head.copy_from_slice(&bytes[cursor..cursor + HEAD_LEN]);
        cursor += HEAD_LEN;
        let mut root_hash = [0u8; HEAD_LEN];
        root_hash.copy_from_slice(&bytes[cursor..cursor + HEAD_LEN]);
        cursor += HEAD_LEN;
        let epoch = take_u32(bytes, &mut cursor);
        let created_ms = take_u64(bytes, &mut cursor);
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&bytes[cursor..cursor + 64]);

        Ok(Self {
            header: BatchHeader {
                device_id,
                seq,
                prev_head: Head(prev_head),
                root_hash,
                epoch,
                created_ms,
            },
            signature,
        })
    }
}

fn take_u64(bytes: &[u8], cursor: &mut usize) -> u64 {
    let end = *cursor + 8;
    let value = u64::from_be_bytes(bytes[*cursor..end].try_into().expect("8 bytes"));
    *cursor = end;
    value
}

fn take_u32(bytes: &[u8], cursor: &mut usize) -> u32 {
    let end = *cursor + 4;
    let value = u32::from_be_bytes(bytes[*cursor..end].try_into().expect("4 bytes"));
    *cursor = end;
    value
}

/// One verified link in a device's chain, persisted by a [`HeadStore`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadRecord {
    pub head: Head,
    pub device_id: String,
    pub seq: u64,
    pub epoch: u32,
}

/// Persistent record of verified heads. Verification appends only on success, so a hostile batch
/// leaves the store byte-identical.
pub trait HeadStore {
    /// The head the client currently trusts.
    fn current(&self) -> Option<&HeadRecord>;

    /// A recorded ancestor, or the implicit genesis record for [`Head::GENESIS`].
    fn get(&self, head: &Head) -> Option<&HeadRecord>;

    /// Record a verified head.
    fn append(&mut self, record: HeadRecord) -> Result<(), ChainError>;
}

/// In-memory store, for a session that has not persisted state yet and for tests.
#[derive(Debug, Default)]
pub struct InMemoryHeadStore {
    current: Option<HeadRecord>,
    records: BTreeMap<Head, HeadRecord>,
}

impl InMemoryHeadStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Preload heads from a persisted client state.
    pub fn from_records(records: impl IntoIterator<Item = HeadRecord>) -> Self {
        let mut store = Self::new();
        for record in records {
            store.current = Some(record.clone());
            store.records.insert(record.head, record);
        }
        store
    }
}

impl HeadStore for InMemoryHeadStore {
    fn current(&self) -> Option<&HeadRecord> {
        self.current.as_ref()
    }

    fn get(&self, head: &Head) -> Option<&HeadRecord> {
        if *head == Head::GENESIS {
            return Some(&GENESIS_RECORD);
        }
        self.records.get(head)
    }

    fn append(&mut self, record: HeadRecord) -> Result<(), ChainError> {
        if self.records.len() >= MAX_CHAIN_RECORDS {
            return Err(ChainError::ChainTooLong {
                limit: MAX_CHAIN_RECORDS,
            });
        }
        self.records.insert(record.head, record.clone());
        self.current = Some(record);
        Ok(())
    }
}

static GENESIS_RECORD: HeadRecord = HeadRecord {
    head: Head::GENESIS,
    device_id: String::new(),
    seq: 0,
    epoch: 0,
};

/// JSON file store. Writes atomically (temp file then rename) so a crash never leaves a half
/// record that would reject the next batch.
#[derive(Debug)]
pub struct FileHeadStore {
    path: PathBuf,
    current: Option<HeadRecord>,
    records: BTreeMap<Head, HeadRecord>,
}

impl FileHeadStore {
    /// File format version. An unknown version is an error, never a silent migration.
    pub const FORMAT_VERSION: u8 = 1;

    pub fn open(path: impl AsRef<Path>) -> Result<Self, ChainError> {
        let path = path.as_ref().to_path_buf();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    current: None,
                    records: BTreeMap::new(),
                });
            }
            Err(error) => return Err(ChainError::Io(error)),
        };
        let file: FileFormat =
            serde_json::from_slice(&bytes).map_err(|error| ChainError::Store(error.to_string()))?;
        if file.version != Self::FORMAT_VERSION {
            return Err(ChainError::Store(format!(
                "head store version {} is not supported; this build writes {}",
                file.version,
                Self::FORMAT_VERSION
            )));
        }
        let records: BTreeMap<Head, HeadRecord> = file
            .records
            .into_iter()
            .map(|record| (record.head, record))
            .collect();
        if let Some(current) = &file.current
            && !records.contains_key(&current.head)
        {
            return Err(ChainError::Store(
                "head store current head is missing from its records".into(),
            ));
        }
        Ok(Self {
            path,
            current: file.current,
            records,
        })
    }

    fn persist(
        &self,
        current: &Option<HeadRecord>,
        records: &BTreeMap<Head, HeadRecord>,
    ) -> Result<(), ChainError> {
        let file = FileFormat {
            version: Self::FORMAT_VERSION,
            current: current.clone(),
            records: records.values().cloned().collect(),
        };
        let bytes =
            serde_json::to_vec(&file).map_err(|error| ChainError::Store(error.to_string()))?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct FileFormat {
    version: u8,
    current: Option<HeadRecord>,
    records: Vec<HeadRecord>,
}

impl HeadStore for FileHeadStore {
    fn current(&self) -> Option<&HeadRecord> {
        self.current.as_ref()
    }

    fn get(&self, head: &Head) -> Option<&HeadRecord> {
        if *head == Head::GENESIS {
            return Some(&GENESIS_RECORD);
        }
        self.records.get(head)
    }

    fn append(&mut self, record: HeadRecord) -> Result<(), ChainError> {
        if self.records.len() >= MAX_CHAIN_RECORDS {
            return Err(ChainError::ChainTooLong {
                limit: MAX_CHAIN_RECORDS,
            });
        }
        let mut records = self.records.clone();
        records.insert(record.head, record.clone());
        let current = Some(record);
        self.persist(&current, &records)?;
        self.records = records;
        self.current = current;
        Ok(())
    }
}

/// Every way a batch can be refused. `Branch` and `Fork` are distinct on purpose: a parent the
/// client has recorded is a detectable fork/rebase, a parent it has never seen is not verifiable
/// as an ancestor at all.
#[derive(Debug, Error)]
pub enum ChainError {
    #[error("batch signature is invalid")]
    Signature,
    #[error("batch version {found} is not supported; this build reads and writes {expected}")]
    UnsupportedVersion { found: u8, expected: u8 },
    #[error("batch device id is {size} bytes, over the {limit}-byte limit")]
    InvalidDeviceId { size: usize, limit: usize },
    #[error("batch encoding is malformed: {reason}")]
    Malformed { reason: &'static str },
    #[error("batch is truncated: needs {needed} bytes, has {found}")]
    Truncated { needed: usize, found: usize },
    #[error("batch carries {extra} trailing bytes")]
    TrailingData { extra: usize },
    #[error("batch seq {found} is not the successor of {expected}")]
    NonSequential { expected: u64, found: u64 },
    #[error("batch branches from recorded head {branch_point:?}, not the current head")]
    Branch { branch_point: Head },
    #[error("batch parent {parent:?} is not the current head and is not a recorded ancestor")]
    Fork { parent: Head },
    #[error("head store holds {limit} records; refusing to grow the chain")]
    ChainTooLong { limit: usize },
    #[error("head store I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("head store error: {0}")]
    Store(String),
}

/// Verify one batch against `store` and, only on success, append its head.
///
/// Checks, in order: signature, parent, sequence. A parent that is the current head must be
/// followed by exactly `current.seq + 1`; a parent that is a recorded ancestor is
/// [`ChainError::Branch`] (a fork or a replay); any other parent is [`ChainError::Fork`].
/// The empty store accepts only `seq == 1` with [`Head::GENESIS`] as parent.
pub fn verify_batch(
    batch: &SignedBatch,
    verifying_key: &VerifyingKey,
    store: &mut impl HeadStore,
) -> Result<Head, ChainError> {
    batch.verify_signature(verifying_key)?;
    let new_head = batch.head()?;

    match store.current().cloned() {
        None => {
            if batch.header.prev_head != Head::GENESIS {
                return Err(ChainError::Fork {
                    parent: batch.header.prev_head,
                });
            }
            if batch.header.seq != 1 {
                return Err(ChainError::NonSequential {
                    expected: 1,
                    found: batch.header.seq,
                });
            }
        }
        Some(current) => {
            if batch.header.prev_head == current.head {
                let expected = current
                    .seq
                    .checked_add(1)
                    .ok_or(ChainError::NonSequential {
                        expected: u64::MAX,
                        found: batch.header.seq,
                    })?;
                if batch.header.seq != expected {
                    return Err(ChainError::NonSequential {
                        expected,
                        found: batch.header.seq,
                    });
                }
            } else if store.get(&batch.header.prev_head).is_some() {
                return Err(ChainError::Branch {
                    branch_point: batch.header.prev_head,
                });
            } else {
                return Err(ChainError::Fork {
                    parent: batch.header.prev_head,
                });
            }
        }
    }

    store.append(HeadRecord {
        head: new_head,
        device_id: batch.header.device_id.clone(),
        seq: batch.header.seq,
        epoch: batch.header.epoch,
    })?;
    Ok(new_head)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn verifying_key() -> VerifyingKey {
        signing_key().verifying_key()
    }

    fn header(seq: u64, prev: Head, root: u8) -> BatchHeader {
        BatchHeader {
            device_id: "device-a".into(),
            seq,
            prev_head: prev,
            root_hash: [root; HEAD_LEN],
            epoch: 0,
            created_ms: 1_000 + seq,
        }
    }

    fn batch(seq: u64, prev: Head, root: u8) -> SignedBatch {
        SignedBatch::sign(header(seq, prev, root), &signing_key()).unwrap()
    }

    fn chain(length: u64) -> (InMemoryHeadStore, Vec<Head>) {
        let mut store = InMemoryHeadStore::new();
        let mut heads = Vec::new();
        let mut prev = Head::GENESIS;
        for seq in 1..=length {
            let signed = batch(seq, prev, seq as u8);
            prev = verify_batch(&signed, &verifying_key(), &mut store).unwrap();
            heads.push(prev);
        }
        (store, heads)
    }

    #[test]
    fn happy_chain_advances_and_records() {
        let (store, heads) = chain(3);
        assert_eq!(store.current().unwrap().head, heads[2]);
        assert_eq!(store.current().unwrap().seq, 3);
        assert!(store.get(&heads[0]).is_some());
        assert!(store.get(&Head::GENESIS).is_some());
    }

    #[test]
    fn tampered_header_fails_signature() {
        let mut signed = batch(1, Head::GENESIS, 1);
        signed.header.root_hash[0] ^= 0x01;
        let mut store = InMemoryHeadStore::new();
        let err = verify_batch(&signed, &verifying_key(), &mut store).unwrap_err();
        assert!(matches!(err, ChainError::Signature));
        assert!(store.current().is_none());
    }

    #[test]
    fn wrong_signature_version_is_typed() {
        let signed = batch(1, Head::GENESIS, 1).to_bytes().unwrap();
        let mut bytes = signed;
        bytes[0] = 99;
        let err = SignedBatch::from_bytes(&bytes).unwrap_err();
        assert!(matches!(
            err,
            ChainError::UnsupportedVersion {
                found: 99,
                expected: CHAIN_VERSION
            }
        ));
    }

    #[test]
    fn non_successor_seq_is_typed() {
        let (mut store, heads) = chain(1);
        let signed = batch(3, heads[0], 9);
        let err = verify_batch(&signed, &verifying_key(), &mut store).unwrap_err();
        assert!(matches!(
            err,
            ChainError::NonSequential {
                expected: 2,
                found: 3
            }
        ));
    }

    #[test]
    fn replay_of_an_old_batch_is_a_branch() {
        let (mut store, heads) = chain(3);
        let replayed = batch(2, heads[0], 2);
        let err = verify_batch(&replayed, &verifying_key(), &mut store).unwrap_err();
        assert!(matches!(
            err,
            ChainError::Branch { branch_point } if branch_point == heads[0]
        ));
        assert_eq!(
            store.current().unwrap().seq,
            3,
            "a refused batch never moves the head"
        );
    }

    #[test]
    fn replay_of_the_first_batch_is_a_branch_at_genesis() {
        let (mut store, _) = chain(3);
        let first = batch(1, Head::GENESIS, 1);
        let err = verify_batch(&first, &verifying_key(), &mut store).unwrap_err();
        assert!(matches!(
            err,
            ChainError::Branch { branch_point } if branch_point == Head::GENESIS
        ));
    }

    #[test]
    fn forked_batch_from_a_recorded_ancestor_is_a_branch() {
        let (mut store, heads) = chain(3);
        let competing = batch(2, heads[0], 0xee);
        let err = verify_batch(&competing, &verifying_key(), &mut store).unwrap_err();
        assert!(matches!(
            err,
            ChainError::Branch { branch_point } if branch_point == heads[0]
        ));
    }

    #[test]
    fn unknown_parent_is_a_fork_not_a_branch() {
        let (mut store, _) = chain(2);
        let unknown = Head::from_bytes([0xab; HEAD_LEN]);
        let signed = batch(3, unknown, 3);
        let err = verify_batch(&signed, &verifying_key(), &mut store).unwrap_err();
        assert!(matches!(
            err,
            ChainError::Fork { parent } if parent == unknown
        ));
    }

    #[test]
    fn first_batch_must_start_at_genesis() {
        let mut store = InMemoryHeadStore::new();
        let err = verify_batch(
            &batch(1, Head::from_bytes([1; HEAD_LEN]), 1),
            &verifying_key(),
            &mut store,
        )
        .unwrap_err();
        assert!(matches!(err, ChainError::Fork { .. }));

        let err =
            verify_batch(&batch(2, Head::GENESIS, 1), &verifying_key(), &mut store).unwrap_err();
        assert!(matches!(
            err,
            ChainError::NonSequential {
                expected: 1,
                found: 2
            }
        ));
    }

    #[test]
    fn transport_round_trip_and_bounds() {
        let signed = batch(1, Head::GENESIS, 1);
        let bytes = signed.to_bytes().unwrap();
        assert_eq!(SignedBatch::from_bytes(&bytes).unwrap(), signed);

        let err = SignedBatch::from_bytes(&bytes[..bytes.len() - 1]).unwrap_err();
        assert!(matches!(err, ChainError::Truncated { .. }));

        let mut extra = bytes;
        extra.push(0);
        let err = SignedBatch::from_bytes(&extra).unwrap_err();
        assert!(matches!(err, ChainError::TrailingData { extra: 1 }));
    }

    #[test]
    fn head_hex_round_trips() {
        let head = Head::from_bytes([0xab; HEAD_LEN]);
        assert_eq!(Head::parse_hex(&head.to_hex()).unwrap(), head);
        assert!(matches!(
            Head::parse_hex("zz"),
            Err(ChainError::Malformed { .. })
        ));
        let json = serde_json::to_string(&head).unwrap();
        assert_eq!(json, format!("\"{}\"", head.to_hex()));
        assert_eq!(serde_json::from_str::<Head>(&json).unwrap(), head);
    }

    #[test]
    fn file_store_survives_a_reopen() {
        let path = std::env::temp_dir().join(format!(
            "alloy-sync-heads-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let heads = {
            let mut store = FileHeadStore::open(&path).unwrap();
            let mut prev = Head::GENESIS;
            let mut heads = Vec::new();
            for seq in 1..=2 {
                let signed = batch(seq, prev, seq as u8);
                prev = verify_batch(&signed, &verifying_key(), &mut store).unwrap();
                heads.push(prev);
            }
            heads
        };

        let mut reopened = FileHeadStore::open(&path).unwrap();
        assert_eq!(reopened.current().unwrap().head, heads[1]);
        assert!(reopened.get(&heads[0]).is_some());
        let third = batch(3, heads[1], 3);
        assert_eq!(
            verify_batch(&third, &verifying_key(), &mut reopened).unwrap(),
            third.head().unwrap()
        );
        drop(reopened);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_store_refuses_an_unknown_format_version() {
        let path = std::env::temp_dir().join(format!(
            "alloy-sync-heads-version-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, r#"{"version":99,"current":null,"records":[]}"#).unwrap();
        let err = FileHeadStore::open(&path).unwrap_err();
        assert!(matches!(err, ChainError::Store(_)));
        let _ = std::fs::remove_file(&path);
    }
}
