//! Authenticated, padded blob codec.
//!
//! A blob is `header || ChaCha20-Poly1305(body)`. The header is versioned and entirely covered by
//! the AEAD's associated data, and the AAD additionally binds the slot the client asked for:
//! `(layer, logical_id, seq, epoch, header bytes)`. Moving an old blob to a new slot therefore
//! fails closed with a typed error instead of decrypting under the new name (ADR 0020 invariant
//! 10).
//!
//! Bodies are zero-padded to a fixed class ([`PAD_CLASS`], 4 KiB) so the server learns only a
//! bucket, never the content length. A declaration that is not the canonical class for its
//! plaintext length, or whose padding bytes are not zero, is rejected: the padding is part of the
//! authenticated body, and a client must not accept a second encoding of the same plaintext.
//! The encoded blob is capped at the server's 1 MiB body limit (invariant 13).

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::RngCore;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::keys::{BlobKey, BlobKeyRing};

/// The only blob version this build writes and reads.
pub const BLOB_VERSION: u8 = 1;

/// Padding granularity: every body is a multiple of this many bytes.
pub const PAD_CLASS: usize = 4096;

/// Hard cap on an encoded blob, matching the server's per-body limit (`413` above it).
pub const MAX_BLOB_BYTES: usize = 1024 * 1024;

/// Cap on a logical id. The server never sees it; the bound is for this codec.
pub const MAX_LOGICAL_ID: usize = 128;

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const FIXED_HEADER_LEN: usize = 1 + 1 + 2 + 8 + 4 + 4 + 4 + NONCE_LEN;
const MAX_HEADER_LEN: usize = FIXED_HEADER_LEN + MAX_LOGICAL_ID;
const MAX_PADDED_LEN: usize = MAX_BLOB_BYTES - MAX_HEADER_LEN - TAG_LEN;
const AAD_DOMAIN: &[u8] = b"alloy-sync-v1/blob/aad";

/// Which store layer a blob belongs to. The value is authenticated, and the decoder checks it
/// against the slot the caller asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Layer {
    Config = 1,
    Tags = 2,
    Notes = 3,
    Roster = 4,
    Escrow = 5,
    Message = 6,
}

impl Layer {
    /// Wire tag.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Config),
            2 => Some(Self::Tags),
            3 => Some(Self::Notes),
            4 => Some(Self::Roster),
            5 => Some(Self::Escrow),
            6 => Some(Self::Message),
            _ => None,
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Config => "config",
            Self::Tags => "tags",
            Self::Notes => "notes",
            Self::Roster => "roster",
            Self::Escrow => "escrow",
            Self::Message => "message",
        })
    }
}

/// Where a blob belongs in the client's namespace. Every field is bound into the AAD, so a blob
/// served at any other slot fails before its decrypt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobAddr {
    pub layer: Layer,
    pub logical_id: String,
    pub seq: u64,
    pub epoch: u32,
}

impl BlobAddr {
    pub fn new(layer: Layer, logical_id: impl Into<String>, seq: u64, epoch: u32) -> Self {
        Self {
            layer,
            logical_id: logical_id.into(),
            seq,
            epoch,
        }
    }
}

/// Every way untrusted blob bytes can be refused. Callers test the variant, never a message.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BlobError {
    #[error(
        "blob version {found} is not supported; this build reads and writes version {expected}"
    )]
    WrongVersion { found: u8, expected: u8 },
    #[error("blob layer {found} does not match the expected slot layer {expected}")]
    WrongLayer { expected: Layer, found: Layer },
    #[error("blob logical id {found:?} does not match the expected slot id {expected:?}")]
    WrongLogicalId { expected: String, found: String },
    #[error("blob seq {found} does not match the expected slot seq {expected}")]
    WrongSeq { expected: u64, found: u64 },
    #[error("blob epoch {found} does not match the expected slot epoch {expected}")]
    WrongEpoch { expected: u32, found: u32 },
    #[error("blob header names unknown layer tag {0}")]
    UnknownLayer(u8),
    #[error("blob logical id is not valid UTF-8")]
    InvalidLogicalId,
    #[error("blob is truncated: needs {needed} bytes, has {found}")]
    Truncated { needed: usize, found: usize },
    #[error("blob declares {field} as {declared}, over the {limit} limit")]
    OversizedDeclaration {
        field: &'static str,
        declared: usize,
        limit: usize,
    },
    #[error("plaintext is {size} bytes, over the {limit}-byte limit")]
    PlaintextTooLarge { size: usize, limit: usize },
    #[error(
        "padding is not canonical: plaintext {plaintext_len} declares padded length {padded_len}"
    )]
    NonCanonicalPadding {
        plaintext_len: usize,
        padded_len: usize,
    },
    #[error("blob carries {extra} trailing bytes after its ciphertext")]
    TrailingData { extra: usize },
    #[error("epoch {epoch} is not in the known key set")]
    UnknownEpoch { epoch: u32 },
    #[error("blob authentication failed")]
    AuthenticationFailed,
}

/// Seal `plaintext` for `addr` under `key`, padding it to the canonical size class.
///
/// The caller chooses the key; [`BlobKeyRing`] is how a client maps an epoch to one.
pub fn seal(key: &BlobKey, addr: &BlobAddr, plaintext: &[u8]) -> Result<Vec<u8>, BlobError> {
    let padded_len = padded_len_for(plaintext.len());
    seal_padded(key, addr, plaintext, padded_len, 0)
}

/// Open a blob that the caller fetched for `addr`.
///
/// The epoch is checked against the slot first and then looked up in `keys`; a blob from an old
/// epoch still opens while the ring holds that epoch.
pub fn open(
    keys: &BlobKeyRing,
    addr: &BlobAddr,
    blob: &[u8],
) -> Result<Zeroizing<Vec<u8>>, BlobError> {
    if blob.len() > MAX_BLOB_BYTES {
        return Err(BlobError::OversizedDeclaration {
            field: "encoded blob",
            declared: blob.len(),
            limit: MAX_BLOB_BYTES,
        });
    }
    let header = parse_header(blob)?;
    if header.layer != addr.layer {
        return Err(BlobError::WrongLayer {
            expected: addr.layer,
            found: header.layer,
        });
    }
    if header.logical_id != addr.logical_id {
        return Err(BlobError::WrongLogicalId {
            expected: addr.logical_id.clone(),
            found: header.logical_id,
        });
    }
    if header.seq != addr.seq {
        return Err(BlobError::WrongSeq {
            expected: addr.seq,
            found: header.seq,
        });
    }
    if header.epoch != addr.epoch {
        return Err(BlobError::WrongEpoch {
            expected: addr.epoch,
            found: header.epoch,
        });
    }
    if header.plaintext_len > header.padded_len
        || header.padded_len != padded_len_for(header.plaintext_len)
    {
        return Err(BlobError::NonCanonicalPadding {
            plaintext_len: header.plaintext_len,
            padded_len: header.padded_len,
        });
    }

    let body = &blob[header.header_len..];
    let needed_body = header.padded_len + TAG_LEN;
    if body.len() < needed_body {
        return Err(BlobError::Truncated {
            needed: header.header_len + needed_body,
            found: blob.len(),
        });
    }
    if body.len() > needed_body {
        return Err(BlobError::TrailingData {
            extra: body.len() - needed_body,
        });
    }

    let key = keys.get(header.epoch).ok_or(BlobError::UnknownEpoch {
        epoch: header.epoch,
    })?;
    let cipher = ChaCha20Poly1305::new(
        &Key::try_from(key.as_bytes().as_slice()).expect("a derived key is exactly 32 bytes"),
    );
    let aad = build_aad(&blob[..header.header_len], addr);
    let decrypted = cipher
        .decrypt(
            &Nonce::try_from(header.nonce.as_slice()).expect("a nonce is exactly 12 bytes"),
            Payload {
                msg: body,
                aad: &aad,
            },
        )
        .map_err(|_| BlobError::AuthenticationFailed)?;
    let mut plaintext = Zeroizing::new(decrypted);
    if plaintext[header.plaintext_len..]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(BlobError::NonCanonicalPadding {
            plaintext_len: header.plaintext_len,
            padded_len: header.padded_len,
        });
    }
    plaintext.truncate(header.plaintext_len);
    Ok(plaintext)
}

/// Canonical padded length for `plaintext_len`: zero for empty input, otherwise the smallest
/// multiple of [`PAD_CLASS`] that holds it.
pub fn padded_len_for(plaintext_len: usize) -> usize {
    if plaintext_len == 0 {
        0
    } else {
        plaintext_len.div_ceil(PAD_CLASS) * PAD_CLASS
    }
}

fn seal_padded(
    key: &BlobKey,
    addr: &BlobAddr,
    plaintext: &[u8],
    padded_len: usize,
    pad_byte: u8,
) -> Result<Vec<u8>, BlobError> {
    if addr.logical_id.len() > MAX_LOGICAL_ID {
        return Err(BlobError::OversizedDeclaration {
            field: "logical id",
            declared: addr.logical_id.len(),
            limit: MAX_LOGICAL_ID,
        });
    }
    if padded_len > MAX_PADDED_LEN {
        return Err(BlobError::PlaintextTooLarge {
            size: plaintext.len(),
            limit: MAX_PADDED_LEN,
        });
    }
    if padded_len < plaintext.len() {
        return Err(BlobError::NonCanonicalPadding {
            plaintext_len: plaintext.len(),
            padded_len,
        });
    }

    let mut padded = Zeroizing::new(vec![pad_byte; padded_len]);
    padded[..plaintext.len()].copy_from_slice(plaintext);

    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let header = build_header(addr, plaintext.len(), padded_len, &nonce);
    if header.len() + padded_len + TAG_LEN > MAX_BLOB_BYTES {
        return Err(BlobError::PlaintextTooLarge {
            size: plaintext.len(),
            limit: MAX_PADDED_LEN,
        });
    }

    let cipher = ChaCha20Poly1305::new(
        &Key::try_from(key.as_bytes().as_slice()).expect("a derived key is exactly 32 bytes"),
    );
    let aad = build_aad(&header, addr);
    let ciphertext = cipher
        .encrypt(
            &Nonce::try_from(nonce.as_slice()).expect("a nonce is exactly 12 bytes"),
            Payload {
                msg: &padded,
                aad: &aad,
            },
        )
        .map_err(|_| BlobError::AuthenticationFailed)?;

    let mut blob = Vec::with_capacity(header.len() + ciphertext.len());
    blob.extend_from_slice(&header);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

struct ParsedHeader {
    layer: Layer,
    logical_id: String,
    seq: u64,
    epoch: u32,
    plaintext_len: usize,
    padded_len: usize,
    nonce: [u8; NONCE_LEN],
    header_len: usize,
}

fn parse_header(blob: &[u8]) -> Result<ParsedHeader, BlobError> {
    if blob.len() < 4 {
        return Err(BlobError::Truncated {
            needed: 4,
            found: blob.len(),
        });
    }
    if blob[0] != BLOB_VERSION {
        return Err(BlobError::WrongVersion {
            found: blob[0],
            expected: BLOB_VERSION,
        });
    }
    let id_len = usize::from(u16::from_be_bytes([blob[2], blob[3]]));
    if id_len > MAX_LOGICAL_ID {
        return Err(BlobError::OversizedDeclaration {
            field: "logical id length",
            declared: id_len,
            limit: MAX_LOGICAL_ID,
        });
    }
    let header_len = FIXED_HEADER_LEN + id_len;
    if blob.len() < header_len {
        return Err(BlobError::Truncated {
            needed: header_len,
            found: blob.len(),
        });
    }

    let layer_raw = blob[1];
    let layer = Layer::from_u8(layer_raw).ok_or(BlobError::UnknownLayer(layer_raw))?;
    let logical_id = std::str::from_utf8(&blob[4..4 + id_len])
        .map_err(|_| BlobError::InvalidLogicalId)?
        .to_owned();

    let mut cursor = 4 + id_len;
    let seq = read_u64(blob, &mut cursor);
    let epoch = read_u32(blob, &mut cursor);
    let plaintext_len = read_u32(blob, &mut cursor) as usize;
    let padded_len = read_u32(blob, &mut cursor) as usize;
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&blob[cursor..cursor + NONCE_LEN]);

    if plaintext_len > MAX_PADDED_LEN || padded_len > MAX_PADDED_LEN {
        return Err(BlobError::OversizedDeclaration {
            field: "declared body length",
            declared: plaintext_len.max(padded_len),
            limit: MAX_PADDED_LEN,
        });
    }

    Ok(ParsedHeader {
        layer,
        logical_id,
        seq,
        epoch,
        plaintext_len,
        padded_len,
        nonce,
        header_len,
    })
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> u64 {
    let end = *cursor + 8;
    let value = u64::from_be_bytes(bytes[*cursor..end].try_into().expect("8 bytes"));
    *cursor = end;
    value
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> u32 {
    let end = *cursor + 4;
    let value = u32::from_be_bytes(bytes[*cursor..end].try_into().expect("4 bytes"));
    *cursor = end;
    value
}

fn build_header(
    addr: &BlobAddr,
    plaintext_len: usize,
    padded_len: usize,
    nonce: &[u8; NONCE_LEN],
) -> Vec<u8> {
    let id = addr.logical_id.as_bytes();
    let mut header = Vec::with_capacity(FIXED_HEADER_LEN + id.len());
    header.push(BLOB_VERSION);
    header.push(addr.layer.as_u8());
    header.extend_from_slice(&(id.len() as u16).to_be_bytes());
    header.extend_from_slice(id);
    header.extend_from_slice(&addr.seq.to_be_bytes());
    header.extend_from_slice(&addr.epoch.to_be_bytes());
    header.extend_from_slice(&(plaintext_len as u32).to_be_bytes());
    header.extend_from_slice(&(padded_len as u32).to_be_bytes());
    header.extend_from_slice(nonce);
    header
}

fn build_aad(header: &[u8], addr: &BlobAddr) -> Vec<u8> {
    let id = addr.logical_id.as_bytes();
    let mut aad = Vec::with_capacity(AAD_DOMAIN.len() + 4 + header.len() + 1 + 2 + id.len() + 12);
    aad.extend_from_slice(AAD_DOMAIN);
    aad.extend_from_slice(&(header.len() as u32).to_be_bytes());
    aad.extend_from_slice(header);
    aad.push(addr.layer.as_u8());
    aad.extend_from_slice(&(id.len() as u16).to_be_bytes());
    aad.extend_from_slice(id);
    aad.extend_from_slice(&addr.seq.to_be_bytes());
    aad.extend_from_slice(&addr.epoch.to_be_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::MasterKey;

    fn addr(layer: Layer, id: &str, seq: u64, epoch: u32) -> BlobAddr {
        BlobAddr::new(layer, id, seq, epoch)
    }

    fn ring(epochs: &[u32]) -> BlobKeyRing {
        BlobKeyRing::new(&MasterKey::from_bytes([9u8; 32]), epochs.iter().copied())
    }

    fn key(epoch: u32) -> BlobKey {
        MasterKey::from_bytes([9u8; 32]).blob_key(epoch)
    }

    #[test]
    fn round_trip_and_size_classes() {
        let ring = ring(&[0]);
        let slot = addr(Layer::Config, "settings", 1, 0);
        let blob = seal(&key(0), &slot, b"hello").unwrap();
        assert_eq!(blob.len(), FIXED_HEADER_LEN + 8 + PAD_CLASS + TAG_LEN);
        assert_eq!(&open(&ring, &slot, &blob).unwrap()[..], b"hello");

        let almost_full = vec![1u8; PAD_CLASS - 1];
        let blob = seal(&key(0), &slot, &almost_full).unwrap();
        assert_eq!(
            blob.len(),
            FIXED_HEADER_LEN + 8 + PAD_CLASS + TAG_LEN,
            "a smaller payload must not shorten the bucket"
        );

        let cross_class = vec![1u8; PAD_CLASS + 1];
        let blob = seal(&key(0), &slot, &cross_class).unwrap();
        assert_eq!(blob.len(), FIXED_HEADER_LEN + 8 + 2 * PAD_CLASS + TAG_LEN);
    }

    #[test]
    fn empty_plaintext_is_canonical() {
        let ring = ring(&[0]);
        let slot = addr(Layer::Notes, "draft", 1, 0);
        let blob = seal(&key(0), &slot, b"").unwrap();
        assert_eq!(&open(&ring, &slot, &blob).unwrap()[..], b"");
    }

    #[test]
    fn old_epoch_still_decrypts_after_rotation() {
        let ring = ring(&[0, 4]);
        let old = addr(Layer::Config, "settings", 1, 0);
        let blob = seal(&key(0), &old, b"before rotation").unwrap();
        assert_eq!(&open(&ring, &old, &blob).unwrap()[..], b"before rotation");
    }

    #[test]
    fn wrong_layer_is_typed() {
        let ring = ring(&[0]);
        let blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        let err = open(&ring, &addr(Layer::Tags, "settings", 1, 0), &blob).unwrap_err();
        assert_eq!(
            err,
            BlobError::WrongLayer {
                expected: Layer::Tags,
                found: Layer::Config
            }
        );
    }

    #[test]
    fn wrong_logical_id_is_typed() {
        let ring = ring(&[0]);
        let blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        let err = open(&ring, &addr(Layer::Config, "theme", 1, 0), &blob).unwrap_err();
        assert!(matches!(err, BlobError::WrongLogicalId { .. }));
    }

    #[test]
    fn wrong_seq_is_typed() {
        let ring = ring(&[0]);
        let blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        let err = open(&ring, &addr(Layer::Config, "settings", 2, 0), &blob).unwrap_err();
        assert_eq!(
            err,
            BlobError::WrongSeq {
                expected: 2,
                found: 1
            }
        );
    }

    #[test]
    fn wrong_epoch_is_typed() {
        let ring = ring(&[0, 1]);
        let blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        let err = open(&ring, &addr(Layer::Config, "settings", 1, 1), &blob).unwrap_err();
        assert_eq!(
            err,
            BlobError::WrongEpoch {
                expected: 1,
                found: 0
            }
        );
    }

    #[test]
    fn unknown_epoch_is_typed() {
        let blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        let err = open(&ring(&[1]), &addr(Layer::Config, "settings", 1, 0), &blob).unwrap_err();
        assert_eq!(err, BlobError::UnknownEpoch { epoch: 0 });
    }

    #[test]
    fn truncation_is_typed() {
        let ring = ring(&[0]);
        let slot = addr(Layer::Config, "settings", 1, 0);
        let blob = seal(&key(0), &slot, b"x").unwrap();
        let err = open(&ring, &slot, &blob[..blob.len() - 1]).unwrap_err();
        assert!(matches!(err, BlobError::Truncated { .. }));
    }

    #[test]
    fn trailing_bytes_are_typed() {
        let ring = ring(&[0]);
        let slot = addr(Layer::Config, "settings", 1, 0);
        let mut blob = seal(&key(0), &slot, b"x").unwrap();
        blob.push(0);
        let err = open(&ring, &slot, &blob).unwrap_err();
        assert_eq!(err, BlobError::TrailingData { extra: 1 });
    }

    #[test]
    fn garbage_is_a_version_error() {
        let err = open(
            &ring(&[0]),
            &addr(Layer::Config, "settings", 1, 0),
            &[0xff; 64],
        )
        .unwrap_err();
        assert_eq!(
            err,
            BlobError::WrongVersion {
                found: 0xff,
                expected: BLOB_VERSION
            }
        );
    }

    #[test]
    fn oversized_logical_id_declaration_is_refused_before_allocation() {
        let mut blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        blob[2..4].copy_from_slice(&u16::MAX.to_be_bytes());
        let err = open(&ring(&[0]), &addr(Layer::Config, "settings", 1, 0), &blob).unwrap_err();
        assert!(matches!(err, BlobError::OversizedDeclaration { .. }));
    }

    #[test]
    fn oversized_body_declaration_is_refused() {
        let mut blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        // padded_len sits after version, layer, id_len, id, seq, epoch and plaintext_len.
        let padded_len_at = FIXED_HEADER_LEN + "settings".len() - NONCE_LEN - 4;
        blob[padded_len_at..padded_len_at + 4]
            .copy_from_slice(&(u32::try_from(MAX_PADDED_LEN + 1).unwrap()).to_be_bytes());
        let err = open(&ring(&[0]), &addr(Layer::Config, "settings", 1, 0), &blob).unwrap_err();
        assert_eq!(
            err,
            BlobError::OversizedDeclaration {
                field: "declared body length",
                declared: MAX_PADDED_LEN + 1,
                limit: MAX_PADDED_LEN
            }
        );
    }

    #[test]
    fn non_canonical_padding_length_is_refused() {
        let mut blob = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), b"x").unwrap();
        let plaintext_len_at = FIXED_HEADER_LEN + "settings".len() - NONCE_LEN - 8;
        let padded_len_at = FIXED_HEADER_LEN + "settings".len() - NONCE_LEN - 4;
        blob[plaintext_len_at..plaintext_len_at + 4].copy_from_slice(&10u32.to_be_bytes());
        blob[padded_len_at..padded_len_at + 4]
            .copy_from_slice(&(2 * PAD_CLASS as u32).to_be_bytes());
        let err = open(&ring(&[0]), &addr(Layer::Config, "settings", 1, 0), &blob).unwrap_err();
        assert_eq!(
            err,
            BlobError::NonCanonicalPadding {
                plaintext_len: 10,
                padded_len: 2 * PAD_CLASS
            }
        );
    }

    #[test]
    fn non_zero_padding_is_refused_after_authentication() {
        let ring = ring(&[0]);
        let slot = addr(Layer::Config, "settings", 1, 0);
        let blob = seal_padded(&key(0), &slot, b"abc", PAD_CLASS, 0xff).unwrap();
        let err = open(&ring, &slot, &blob).unwrap_err();
        assert_eq!(
            err,
            BlobError::NonCanonicalPadding {
                plaintext_len: 3,
                padded_len: PAD_CLASS
            }
        );
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let ring = ring(&[0]);
        let slot = addr(Layer::Config, "settings", 1, 0);
        let mut blob = seal(&key(0), &slot, b"x").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        let err = open(&ring, &slot, &blob).unwrap_err();
        assert_eq!(err, BlobError::AuthenticationFailed);
    }

    #[test]
    fn oversized_encoded_blob_is_refused_before_parsing() {
        let err = open(
            &ring(&[0]),
            &addr(Layer::Config, "settings", 1, 0),
            &vec![0u8; MAX_BLOB_BYTES + 1],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            BlobError::OversizedDeclaration {
                field: "encoded blob",
                ..
            }
        ));
    }

    #[test]
    fn sealing_a_plaintext_over_the_cap_is_typed() {
        let too_big = vec![0u8; MAX_PADDED_LEN + 1];
        let err = seal(&key(0), &addr(Layer::Config, "settings", 1, 0), &too_big).unwrap_err();
        assert!(matches!(err, BlobError::PlaintextTooLarge { .. }));
    }
}
