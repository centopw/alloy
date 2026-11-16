//! Master key hierarchy for the sync server (ADR 0020, slice B).
//!
//! The first device generates one 32-byte [`MasterKey`]. Every key used on the wire is derived
//! from it with HKDF-SHA256 and a version-labelled info string, so a version bump can never reuse
//! the same key material:
//!
//! | Material | Info |
//! |---|---|
//! | blob AEAD key (epoch `e`) | `alloy-sync-v1/blob/epoch/e` |
//! | blob address key (epoch `e`) | `alloy-sync-v1/addr/epoch/e` |
//! | roster signing material | `alloy-sync-v1/roster` |
//! | recovery wrapper material | `alloy-sync-v1/recovery` |
//!
//! The types are deliberately not interchangeable: [`BlobKey`] and [`AddrKey`] are distinct
//! structs, so an address key cannot be passed where an AEAD key is expected. None of them is
//! `Clone`, none serializes, and every `Debug` implementation is redacted. The key bytes are only
//! reachable through `as_bytes`, which is crate-private.
//!
//! Rotation (invariant 14) is an epoch bump. Old epochs stay readable: the client keeps
//! [`BlobKeyRing`] entries for every known epoch, and new blobs are sealed under the newest one.

use std::collections::BTreeMap;
use std::fmt;

use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroizing;

/// Length of the master key and of every derived key.
pub const MASTER_KEY_LEN: usize = 32;

/// Version label embedded in every derivation info string.
pub const KEY_VERSION: &str = "alloy-sync-v1";

/// Salt for the master-key HKDF. The master key is already uniformly random, so the salt is a
/// fixed domain separator rather than a per-account value: it keeps this derivation distinct from
/// any other use of the same master bytes an embedding application might make.
const HKDF_SALT: &[u8] = b"alloy-sync-v1/master";

/// The account's root secret. Wiped on drop; never serialized by this crate.
pub struct MasterKey(Zeroizing<[u8; MASTER_KEY_LEN]>);

impl MasterKey {
    /// Draw a fresh master key from the OS RNG.
    pub fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; MASTER_KEY_LEN]);
        rand::rng().fill_bytes(&mut bytes[..]);
        Self(bytes)
    }

    /// Rebuild a master key from bytes that a secret store returned. The caller owns the source
    /// copy; this value wipes its own copy on drop.
    pub fn from_bytes(bytes: [u8; MASTER_KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The raw key, for sealing it into the secret store or the recovery wrapper. Callers must not
    /// keep the returned reference after the value drops.
    pub fn expose_secret(&self) -> &[u8; MASTER_KEY_LEN] {
        &self.0
    }

    /// AEAD key for blob bodies written under `epoch`.
    pub fn blob_key(&self, epoch: u32) -> BlobKey {
        BlobKey(derive(self, &format!("{KEY_VERSION}/blob/epoch/{epoch}")))
    }

    /// Address key for the HMAC that names blobs written under `epoch`.
    pub fn addr_key(&self, epoch: u32) -> AddrKey {
        AddrKey(derive(self, &format!("{KEY_VERSION}/addr/epoch/{epoch}")))
    }

    /// Material the device-roster signing key is derived from.
    pub fn roster_key(&self) -> RosterKey {
        RosterKey(derive(self, &format!("{KEY_VERSION}/roster")))
    }

    /// Material the recovery wrapper is built around.
    pub fn recovery_key(&self) -> RecoveryKey {
        RecoveryKey(derive(self, &format!("{KEY_VERSION}/recovery")))
    }
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}

macro_rules! derived_key {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        pub struct $name(Zeroizing<[u8; MASTER_KEY_LEN]>);

        impl $name {
            /// The derived bytes. Public so the roster, recovery and address layers can use their
            /// own key without exposing the master key; `Debug` stays redacted.
            pub fn as_bytes(&self) -> &[u8; MASTER_KEY_LEN] {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

derived_key! {
    /// ChaCha20-Poly1305 key for one epoch's blob bodies. Not an address key by type.
    BlobKey
}

derived_key! {
    /// HMAC-SHA256 key that names blobs for one epoch. Not an AEAD key by type.
    AddrKey
}

derived_key! {
    /// Material behind the device-roster signature.
    RosterKey
}

derived_key! {
    /// Material behind the recovery wrapper.
    RecoveryKey
}

fn derive(master: &MasterKey, info: &str) -> Zeroizing<[u8; MASTER_KEY_LEN]> {
    let hkdf = Hkdf::<Sha256>::new(Some(HKDF_SALT), &master.0[..]);
    let mut out = Zeroizing::new([0u8; MASTER_KEY_LEN]);
    hkdf.expand(info.as_bytes(), &mut out[..])
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    out
}

/// The blob keys a client can currently decrypt: exactly `{blob_key(e) | e in known_epochs}`.
///
/// Decoding a blob looks the key up by the epoch that the authenticated header names, so a blob
/// written before a rotation still opens while new blobs use the newest key.
pub struct BlobKeyRing {
    keys: BTreeMap<u32, BlobKey>,
}

impl BlobKeyRing {
    /// Derive keys for every known epoch.
    pub fn new(master: &MasterKey, epochs: impl IntoIterator<Item = u32>) -> Self {
        let mut ring = Self {
            keys: BTreeMap::new(),
        };
        for epoch in epochs {
            ring.insert(master, epoch);
        }
        ring
    }

    /// Add an epoch without dropping the older keys.
    pub fn insert(&mut self, master: &MasterKey, epoch: u32) {
        self.keys.insert(epoch, master.blob_key(epoch));
    }

    /// The key for `epoch`, if the client knows it.
    pub fn get(&self, epoch: u32) -> Option<&BlobKey> {
        self.keys.get(&epoch)
    }

    /// Whether `epoch` is in the known set.
    pub fn contains(&self, epoch: u32) -> bool {
        self.keys.contains_key(&epoch)
    }

    /// Known epochs in ascending order.
    pub fn epochs(&self) -> impl Iterator<Item = u32> + '_ {
        self.keys.keys().copied()
    }
}

impl fmt::Debug for BlobKeyRing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlobKeyRing")
            .field("epochs", &self.epochs().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master() -> MasterKey {
        MasterKey::from_bytes([7u8; MASTER_KEY_LEN])
    }

    #[test]
    fn derivation_is_deterministic_and_domain_separated() {
        let base = master();
        assert_eq!(base.blob_key(0).as_bytes(), master().blob_key(0).as_bytes());
        assert_ne!(base.blob_key(0).as_bytes(), base.blob_key(1).as_bytes());
        assert_ne!(base.blob_key(0).as_bytes(), base.addr_key(0).as_bytes());
        assert_ne!(base.roster_key().as_bytes(), base.recovery_key().as_bytes());
    }

    #[test]
    fn a_new_epoch_derives_a_different_blob_key_from_the_same_master() {
        let master = master();
        let ring = BlobKeyRing::new(&master, [0, 1]);
        assert_ne!(
            ring.get(0).unwrap().as_bytes(),
            ring.get(1).unwrap().as_bytes()
        );
        assert_eq!(ring.epochs().collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn debug_never_prints_key_bytes() {
        let text = format!(
            "{:?} {:?} {:?}",
            master(),
            master().blob_key(0),
            BlobKeyRing::new(&master(), [0])
        );
        assert!(!text.contains("7, 7, 7"));
        assert!(text.contains("<redacted>"));
    }
}
