//! A hostile fake server, in process and with no network, plus the client-side tests that must
//! refuse everything it does (ADR 0020 invariants 4, 5, 10, 11 and 13).
//!
//! Each attack gets its own test and asserts the exact error variant, not a message. The fake
//! server can rewrite, replay, roll back, fork, relabel, truncate and serve garbage; the client
//! under test is a [`verify_batch`] call plus a [`ClientState`] snapshot.

use std::collections::BTreeMap;

use alloy_storage::config_bundle::{BUNDLE_SCHEMA, ChatKey, ConfigBundle, TagSet};
use alloy_sync::blob::{BlobAddr, BlobError, Layer, MAX_BLOB_BYTES, open, seal};
use alloy_sync::chain::{
    BatchHeader, ChainError, Head, HeadStore, InMemoryHeadStore, SignedBatch, verify_batch,
};
use alloy_sync::client_state::ClientState;
use alloy_sync::keys::{BlobKeyRing, MASTER_KEY_LEN, MasterKey};
use ed25519_dalek::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

const DEVICE_ID: &str = "device-a";

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn verifying_key() -> VerifyingKey {
    signing_key().verifying_key()
}

/// In-process attacker. It enforces no policy at all: whatever an upload, a fork, a rollback or a
/// relabel sets is what it serves next, which is exactly the server root adversary in ADR 0020.
#[derive(Default)]
struct FakeServer {
    /// Raw batch bytes currently advertised as `head`.
    head: Option<Vec<u8>>,
    /// Every batch ever uploaded, oldest first.
    history: Vec<Vec<u8>>,
    /// Ciphertexts keyed by the address the server is told to serve them under.
    blobs: BTreeMap<(Layer, String), Vec<u8>>,
}

impl FakeServer {
    fn new() -> Self {
        Self::default()
    }

    fn upload(&mut self, batch: &SignedBatch) {
        let bytes = batch.to_bytes().unwrap();
        self.head = Some(bytes.clone());
        self.history.push(bytes);
    }

    fn serve_head(&self) -> SignedBatch {
        SignedBatch::from_bytes(self.head.as_deref().expect("server has a head")).unwrap()
    }

    fn roll_back_to(&mut self, index: usize) {
        self.head = Some(self.history[index].clone());
    }

    fn put_blob(&mut self, addr: &BlobAddr, bytes: Vec<u8>) {
        self.blobs
            .insert((addr.layer, addr.logical_id.clone()), bytes);
    }

    fn move_blob(&mut self, from: &BlobAddr, to: &BlobAddr) {
        let bytes = self
            .blobs
            .remove(&(from.layer, from.logical_id.clone()))
            .expect("source blob exists");
        self.put_blob(to, bytes);
    }

    fn truncate_blob(&mut self, addr: &BlobAddr, by: usize) {
        let bytes = self
            .blobs
            .get_mut(&(addr.layer, addr.logical_id.clone()))
            .expect("blob exists");
        bytes.truncate(bytes.len() - by);
    }

    fn tamper_blob(&mut self, addr: &BlobAddr) {
        let bytes = self
            .blobs
            .get_mut(&(addr.layer, addr.logical_id.clone()))
            .expect("blob exists");
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
    }

    fn serve_blob(&self, addr: &BlobAddr) -> &[u8] {
        self.blobs
            .get(&(addr.layer, addr.logical_id.clone()))
            .expect("blob exists")
    }
}

/// The client under attack: a verified-head store plus the cursor it would persist.
struct Client {
    store: InMemoryHeadStore,
    state: ClientState,
}

impl Client {
    fn new() -> Self {
        Self {
            store: InMemoryHeadStore::new(),
            state: ClientState::new([0]),
        }
    }

    fn accept_head(&mut self, server: &FakeServer) -> Result<Head, ChainError> {
        let batch = server.serve_head();
        let head = verify_batch(&batch, &verifying_key(), &mut self.store)?;
        self.state
            .record_head(DEVICE_ID, batch.header.seq, head, batch.header.epoch);
        Ok(head)
    }

    fn snapshot(&self) -> Vec<u8> {
        self.state.to_json().unwrap()
    }
}

fn header(seq: u64, prev: Head, root: [u8; 32]) -> BatchHeader {
    BatchHeader {
        device_id: DEVICE_ID.into(),
        seq,
        prev_head: prev,
        root_hash: root,
        epoch: 0,
        created_ms: 1_000 + seq,
    }
}

fn batch(seq: u64, prev: Head, root: u8) -> SignedBatch {
    SignedBatch::sign(header(seq, prev, [root; 32]), &signing_key()).unwrap()
}

fn ring(epochs: &[u32]) -> BlobKeyRing {
    BlobKeyRing::new(
        &MasterKey::from_bytes([9u8; MASTER_KEY_LEN]),
        epochs.iter().copied(),
    )
}

fn key(epoch: u32) -> alloy_sync::keys::BlobKey {
    MasterKey::from_bytes([9u8; MASTER_KEY_LEN]).blob_key(epoch)
}

fn addr(layer: Layer, id: &str, seq: u64, epoch: u32) -> BlobAddr {
    BlobAddr::new(layer, id, seq, epoch)
}

fn sample_bundle() -> ConfigBundle {
    ConfigBundle {
        schema: BUNDLE_SCHEMA,
        exported_at_ms: 1_700_000_000_000,
        device_label: "hostile-test".into(),
        settings: serde_json::json!({"theme": "dark", "density": "compact"}),
        tags: BTreeMap::from([(
            ChatKey::new("telegram", "42"),
            TagSet::new(
                ["work".to_string(), "urgent".to_string()],
                1_700_000_000_001,
            ),
        )]),
        catalogue: serde_json::json!({"version": 1}),
        backgrounds: Vec::new(),
        theme: None,
    }
}

// ---------------------------------------------------------------------------------------------
// Chain attacks
// ---------------------------------------------------------------------------------------------

#[test]
fn attack_replayed_batch_is_refused_as_branch_and_state_kept() {
    let mut server = FakeServer::new();
    let mut client = Client::new();

    let first = batch(1, Head::GENESIS, 1);
    server.upload(&first);
    let head1 = client.accept_head(&server).unwrap();
    let second = batch(2, head1, 2);
    server.upload(&second);
    let head2 = client.accept_head(&server).unwrap();
    server.upload(&batch(3, head2, 3));
    client.accept_head(&server).unwrap();

    let before = client.snapshot();
    server.roll_back_to(1); // replay the successor of head1
    let err = client.accept_head(&server).unwrap_err();
    assert!(
        matches!(err, ChainError::Branch { branch_point } if branch_point == head1),
        "expected a branch at head1, got {err:?}"
    );
    assert_eq!(client.snapshot(), before, "the cursor must not move");
    assert_eq!(client.store.current().unwrap().seq, 3);
}

#[test]
fn attack_rolled_back_head_is_refused_and_local_head_kept() {
    let mut server = FakeServer::new();
    let mut client = Client::new();
    let mut prev = Head::GENESIS;
    for seq in 1..=4u64 {
        let signed = batch(seq, prev, seq as u8);
        server.upload(&signed);
        prev = client.accept_head(&server).unwrap();
    }

    let before = client.snapshot();
    server.roll_back_to(0); // serve the very first batch again
    let err = client.accept_head(&server).unwrap_err();
    assert!(
        matches!(err, ChainError::Branch { branch_point } if branch_point == Head::GENESIS),
        "expected a branch at genesis, got {err:?}"
    );
    assert_eq!(client.store.current().unwrap().head, prev);
    assert_eq!(client.snapshot(), before);
}

#[test]
fn attack_forked_batch_with_unknown_parent_is_refused_as_fork() {
    let mut server = FakeServer::new();
    let mut client = Client::new();
    let first = batch(1, Head::GENESIS, 1);
    server.upload(&first);
    client.accept_head(&server).unwrap();

    let unknown = Head::from_bytes([0xab; 32]);
    server.upload(&batch(2, unknown, 0xee));
    let before = client.snapshot();
    let err = client.accept_head(&server).unwrap_err();
    assert!(
        matches!(err, ChainError::Fork { parent } if parent == unknown),
        "expected an unknown-parent fork, got {err:?}"
    );
    assert_eq!(client.store.current().unwrap().seq, 1);
    assert_eq!(client.snapshot(), before);
}

#[test]
fn attack_signature_forgery_is_refused() {
    let mut server = FakeServer::new();
    let mut client = Client::new();
    let mut forged = batch(1, Head::GENESIS, 1);
    forged.header.root_hash[0] ^= 0x01;
    server.upload(&forged);
    let err = client.accept_head(&server).unwrap_err();
    assert!(matches!(err, ChainError::Signature));
    assert!(client.store.current().is_none());
}

// ---------------------------------------------------------------------------------------------
// Blob attacks
// ---------------------------------------------------------------------------------------------

#[test]
fn attack_relabelled_layer_is_refused() {
    let mut server = FakeServer::new();
    let original = addr(Layer::Config, "settings", 1, 0);
    server.put_blob(&original, seal(&key(0), &original, b"config").unwrap());

    let other = addr(Layer::Tags, "settings", 1, 0);
    server.move_blob(&original, &other);
    let err = open(&ring(&[0]), &other, server.serve_blob(&other)).unwrap_err();
    assert_eq!(
        err,
        BlobError::WrongLayer {
            expected: Layer::Tags,
            found: Layer::Config
        }
    );
}

#[test]
fn attack_relabelled_logical_id_is_refused() {
    let mut server = FakeServer::new();
    let original = addr(Layer::Config, "settings", 1, 0);
    server.put_blob(&original, seal(&key(0), &original, b"config").unwrap());

    let other = addr(Layer::Config, "theme", 1, 0);
    server.move_blob(&original, &other);
    let err = open(&ring(&[0]), &other, server.serve_blob(&other)).unwrap_err();
    assert!(
        matches!(err, BlobError::WrongLogicalId { .. }),
        "got {err:?}"
    );
}

#[test]
fn attack_replayed_blob_into_a_new_seq_slot_is_refused() {
    let mut server = FakeServer::new();
    let old_slot = addr(Layer::Config, "settings", 1, 0);
    server.put_blob(&old_slot, seal(&key(0), &old_slot, b"config").unwrap());

    let new_slot = addr(Layer::Config, "settings", 2, 0);
    server.move_blob(&old_slot, &new_slot);
    let err = open(&ring(&[0]), &new_slot, server.serve_blob(&new_slot)).unwrap_err();
    assert_eq!(
        err,
        BlobError::WrongSeq {
            expected: 2,
            found: 1
        }
    );
}

#[test]
fn attack_blob_moved_across_epochs_is_refused() {
    let mut server = FakeServer::new();
    let old_slot = addr(Layer::Config, "settings", 1, 0);
    server.put_blob(&old_slot, seal(&key(0), &old_slot, b"config").unwrap());

    let new_slot = addr(Layer::Config, "settings", 1, 1);
    server.move_blob(&old_slot, &new_slot);
    let err = open(&ring(&[0, 1]), &new_slot, server.serve_blob(&new_slot)).unwrap_err();
    assert_eq!(
        err,
        BlobError::WrongEpoch {
            expected: 1,
            found: 0
        }
    );
}

#[test]
fn attack_truncated_blob_is_refused() {
    let mut server = FakeServer::new();
    let slot = addr(Layer::Config, "settings", 1, 0);
    server.put_blob(&slot, seal(&key(0), &slot, b"config").unwrap());
    server.truncate_blob(&slot, 7);
    let err = open(&ring(&[0]), &slot, server.serve_blob(&slot)).unwrap_err();
    assert!(matches!(err, BlobError::Truncated { .. }), "got {err:?}");
}

#[test]
fn attack_tampered_ciphertext_is_refused() {
    let mut server = FakeServer::new();
    let slot = addr(Layer::Config, "settings", 1, 0);
    server.put_blob(&slot, seal(&key(0), &slot, b"config").unwrap());
    server.tamper_blob(&slot);
    let err = open(&ring(&[0]), &slot, server.serve_blob(&slot)).unwrap_err();
    assert_eq!(err, BlobError::AuthenticationFailed);
}

#[test]
fn attack_buckets_of_garbage_are_bounded_and_refused() {
    let slot = addr(Layer::Config, "settings", 1, 0);
    let ring = ring(&[0]);

    let mut oversized_id = vec![0u8; 64];
    oversized_id[0] = alloy_sync::blob::BLOB_VERSION;
    oversized_id[1] = Layer::Config.as_u8();
    oversized_id[2..4].copy_from_slice(&u16::MAX.to_be_bytes());

    for (bytes, expected) in [
        (Vec::new(), "truncated"),
        (vec![0u8; 1], "truncated"),
        (vec![0xff; 64], "wrong version"),
        (oversized_id, "oversized declaration"),
        (vec![0u8; MAX_BLOB_BYTES + 1], "oversized blob"),
    ] {
        let mut server = FakeServer::new();
        server.put_blob(&slot, bytes);
        let err = open(&ring, &slot, server.serve_blob(&slot)).unwrap_err();
        let ok = match expected {
            "truncated" => matches!(err, BlobError::Truncated { .. }),
            "wrong version" => matches!(err, BlobError::WrongVersion { .. }),
            "oversized declaration" => matches!(err, BlobError::OversizedDeclaration { .. }),
            "oversized blob" => matches!(
                err,
                BlobError::OversizedDeclaration {
                    field: "encoded blob",
                    ..
                }
            ),
            other => panic!("unknown expectation {other}"),
        };
        assert!(ok, "garbage class {expected} got {err:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// End to end: a real ConfigBundle through blobs and the chain, then attacked
// ---------------------------------------------------------------------------------------------

#[test]
fn attack_config_bundle_stream_keeps_local_state_and_reports_the_violation() {
    let master = MasterKey::from_bytes([3u8; MASTER_KEY_LEN]);
    let keys = BlobKeyRing::new(&master, [0]);
    let bundle = sample_bundle();
    let bundle_json = bundle.to_json().unwrap();

    let mut server = FakeServer::new();
    let settings_addr = addr(Layer::Config, "settings", 1, 0);
    let tags_addr = addr(Layer::Tags, "tags", 1, 0);
    let settings_blob = seal(&master.blob_key(0), &settings_addr, &bundle_json).unwrap();
    let tags_blob = seal(&master.blob_key(0), &tags_addr, b"work,urgent").unwrap();
    server.put_blob(&settings_addr, settings_blob.clone());
    server.put_blob(&tags_addr, tags_blob.clone());

    let mut client = Client::new();
    let mut root1 = [0u8; 32];
    root1.copy_from_slice(&Sha256::digest(&settings_blob));
    let mut root2 = [0u8; 32];
    root2.copy_from_slice(&Sha256::digest(&tags_blob));
    let first = SignedBatch::sign(header(1, Head::GENESIS, root1), &signing_key()).unwrap();
    server.upload(&first);
    let head1 = client.accept_head(&server).unwrap();
    let second = SignedBatch::sign(header(2, head1, root2), &signing_key()).unwrap();
    server.upload(&second);
    let head2 = client.accept_head(&server).unwrap();

    // The honest read: both blobs decode and the real bundle round-trips.
    let decoded = open(
        &client.state.blob_keys(&master),
        &settings_addr,
        server.serve_blob(&settings_addr),
    )
    .unwrap();
    assert_eq!(ConfigBundle::from_json(&decoded).unwrap(), bundle);
    assert_eq!(
        &open(&keys, &tags_addr, server.serve_blob(&tags_addr)).unwrap()[..],
        b"work,urgent"
    );

    let before = client.snapshot();

    // Attack 1: the server relabels the config blob into the tags layer.
    server.move_blob(&settings_addr, &tags_addr);
    let err = open(
        &client.state.blob_keys(&master),
        &tags_addr,
        server.serve_blob(&tags_addr),
    )
    .unwrap_err();
    assert!(matches!(err, BlobError::WrongLayer { .. }), "got {err:?}");
    assert_eq!(client.snapshot(), before);

    // Attack 2: it serves a forked batch whose parent it cannot justify.
    server.upload(&batch(3, Head::from_bytes([0x5a; 32]), 3));
    let err = client.accept_head(&server).unwrap_err();
    assert!(matches!(err, ChainError::Fork { .. }), "got {err:?}");
    assert_eq!(client.store.current().unwrap().head, head2);
    assert_eq!(client.snapshot(), before);

    // Attack 3: it rolls the head back to the first batch.
    server.roll_back_to(0);
    let err = client.accept_head(&server).unwrap_err();
    assert!(
        matches!(err, ChainError::Branch { branch_point } if branch_point == Head::GENESIS),
        "got {err:?}"
    );
    assert_eq!(client.store.current().unwrap().head, head2);
    assert_eq!(client.snapshot(), before);

    // The client's local copy still opens: the attack cost it no plaintext, no head and no cursor.
    let decoded = open(&keys, &settings_addr, &settings_blob).unwrap();
    assert_eq!(ConfigBundle::from_json(&decoded).unwrap(), bundle);
    assert_eq!(
        open(&keys, &tags_addr, &tags_blob).unwrap().as_slice(),
        b"work,urgent"
    );

    let persisted = ClientState::from_json(&client.snapshot()).unwrap();
    assert_eq!(persisted.cursor(DEVICE_ID).unwrap().head, head2);
}
