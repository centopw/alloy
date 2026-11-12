//! The session blob the host stores, and the peer cache it carries.
//!
//! grammers keeps everything needed to talk to Telegram again in a [`MemorySession`]: the home
//! datacenter, one authorization key per datacenter, the peer cache it needs to resolve peers, and
//! the update-state checkpoint. That state is not `Serialize` itself, so [`snapshot`] copies it
//! field by field into [`SessionDataJson`], and [`import`] puts it back.
//!
//! The envelope also carries this connector’s own peer cache (connector id → access hash) so that
//! [`crate::TelegramConnector::restore`] can resolve the chat ids the host stored earlier without
//! listing dialogs again.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use alloy_connector::ConnectorError;
use grammers_session::Session;
use grammers_session::SessionData;
use grammers_session::storages::{MemorySession, MemorySessionError};
use grammers_session::types::{DcOption, PeerId, PeerInfo, UpdatesState};
use serde::{Deserialize, Serialize};

use crate::mapping::{self, PeerKindTag};

/// Version of the blob layout. Anything else is rejected instead of guessed at.
pub(crate) const SESSION_VERSION: u32 = 1;

/// One peer the connector can resolve without asking Telegram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CachedPeer {
    /// Telegram’s bare id.
    pub bare_id: i64,
    /// Which kind of peer this is.
    pub kind: PeerKindTag,
    /// Access hash, or `0` when Telegram considers the peer ambient (contacts, small groups).
    pub auth: i64,
}

/// Connector id → resolvable peer.
pub(crate) type PeerCache = BTreeMap<String, CachedPeer>;

/// The account a blob belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SelfProfile {
    /// Connector id of the account (`u123`).
    pub remote_user_id: String,
    /// Telegram’s bare user id.
    pub bare_id: i64,
    /// Display name Telegram reports for the account.
    pub display_name: String,
    /// The account’s own profile picture, when Telegram reports one.
    #[serde(default)]
    pub avatar: Option<alloy_connector::MediaRef>,
}

/// The serialized session state, mirroring [`SessionData`] field for field.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SessionDataJson {
    /// Datacenter the account belongs to.
    pub home_dc: i32,
    /// Every datacenter we have an address or key for.
    pub dc_options: Vec<DcOption>,
    /// Peer information grammers had cached.
    pub peer_infos: Vec<PeerInfo>,
    /// Update-state checkpoint, so a restart does not lose events.
    pub updates_state: UpdatesState,
}

/// The whole blob.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Envelope {
    /// Layout version, always [`SESSION_VERSION`] for blobs this connector writes.
    pub v: u32,
    /// grammers session state.
    pub data: SessionDataJson,
    /// This connector’s peer cache.
    #[serde(default)]
    pub peers: PeerCache,
    /// The account the blob belongs to, when it was produced by a login.
    #[serde(default)]
    pub myself: Option<SelfProfile>,
}

/// Serializes a blob.
pub(crate) fn encode(envelope: &Envelope) -> Result<Vec<u8>, ConnectorError> {
    serde_json::to_vec(envelope).map_err(|error| ConnectorError::Protocol {
        message: format!("cannot serialize the telegram session: {error}"),
    })
}

/// Parses a blob, rejecting every layout this crate does not write.
///
/// A caller cannot repair a blob, so a wrong version or malformed body is reported as
/// [`ConnectorError::Unauthorized`]: the host has to run the login flow again.
pub(crate) fn decode(bytes: &[u8]) -> Result<Envelope, ConnectorError> {
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| ConnectorError::Unauthorized)?;
    if envelope.v != SESSION_VERSION {
        return Err(ConnectorError::Unauthorized);
    }
    Ok(envelope)
}

/// Reads the current state of `session` into an envelope.
pub(crate) async fn snapshot(
    session: &MemorySession,
    peers: &PeerCache,
    myself: Option<&SelfProfile>,
    dc_ids: &BTreeSet<i32>,
) -> Result<Envelope, ConnectorError> {
    let home_dc = session.home_dc_id().map_err(session_error)?;

    let mut ids: BTreeSet<i32> = dc_ids.clone();
    ids.insert(home_dc);
    let mut dc_options = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(option) = session.dc_option(id).map_err(session_error)? {
            dc_options.push(option);
        }
    }

    let mut peer_infos = Vec::with_capacity(peers.len());
    let mut seen: HashSet<PeerId> = HashSet::with_capacity(peers.len());
    for cached in peers.values() {
        let Some(id) = mapping::peer_id(cached.kind, cached.bare_id) else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        if let Some(info) = session.peer(id).await.map_err(session_error)? {
            peer_infos.push(info);
        }
    }

    let updates_state = session.updates_state().await.map_err(session_error)?;

    Ok(Envelope {
        v: SESSION_VERSION,
        data: SessionDataJson {
            home_dc,
            dc_options,
            peer_infos,
            updates_state,
        },
        peers: peers.clone(),
        myself: myself.cloned(),
    })
}

/// Replaces the contents of `session` with the envelope’s state.
pub(crate) async fn import(
    session: &MemorySession,
    envelope: &Envelope,
) -> Result<(), ConnectorError> {
    let data = SessionData {
        home_dc: envelope.data.home_dc,
        dc_options: envelope
            .data
            .dc_options
            .iter()
            .cloned()
            .map(|option| (option.id, option))
            .collect(),
        peer_infos: envelope
            .data
            .peer_infos
            .iter()
            .cloned()
            .map(|info| (info.id(), info))
            .collect(),
        updates_state: envelope.data.updates_state.clone(),
    };
    data.import_to(session).await.map_err(session_error)
}

/// Drops every credential and cached peer, as logout requires.
pub(crate) async fn wipe(session: &MemorySession) -> Result<(), ConnectorError> {
    SessionData::default()
        .import_to(session)
        .await
        .map_err(session_error)
}

/// Every datacenter the envelope knows about, for later snapshots.
pub(crate) fn datacenters(envelope: &Envelope) -> BTreeSet<i32> {
    let mut ids: BTreeSet<i32> = envelope
        .data
        .dc_options
        .iter()
        .map(|option| option.id)
        .collect();
    ids.insert(envelope.data.home_dc);
    ids
}

/// A [`MemorySession`] failure is never a caller problem: it means the in-process session could
/// not be read, which leaves nothing sensible to return.
pub(crate) fn session_error(error: MemorySessionError) -> ConnectorError {
    ConnectorError::Protocol {
        message: format!("telegram session error: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use grammers_session::types::{PeerAuth, PeerInfo};

    use super::*;

    fn empty_envelope() -> Envelope {
        Envelope {
            v: SESSION_VERSION,
            data: SessionDataJson {
                home_dc: 2,
                dc_options: Vec::new(),
                peer_infos: Vec::new(),
                updates_state: UpdatesState::default(),
            },
            peers: PeerCache::new(),
            myself: None,
        }
    }

    #[tokio::test]
    async fn envelope_round_trips_through_the_session() {
        let session = MemorySession::default();
        let mut peers = PeerCache::new();
        peers.insert(
            "u7".to_string(),
            CachedPeer {
                bare_id: 7,
                kind: PeerKindTag::User,
                auth: -1_234,
            },
        );
        session
            .cache_peer(PeerInfo::User {
                id: 7,
                auth: Some(PeerAuth::from_hash(-1_234)),
                bot: Some(false),
                is_self: Some(false),
            })
            .await
            .expect("cache self");
        let myself = SelfProfile {
            remote_user_id: "u7".to_string(),
            bare_id: 7,
            display_name: "Ada".to_string(),
            avatar: None,
        };

        let mut dc_ids = BTreeSet::new();
        dc_ids.insert(2);
        let first = snapshot(&session, &peers, Some(&myself), &dc_ids)
            .await
            .expect("snapshot");
        let bytes = encode(&first).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(decoded.data.home_dc, first.data.home_dc);
        assert_eq!(decoded.peers, peers);
        assert_eq!(decoded.myself, Some(myself.clone()));

        let restored = MemorySession::default();
        import(&restored, &decoded).await.expect("import");

        assert_eq!(restored.home_dc_id().expect("home dc"), first.data.home_dc);
        let peer = restored
            .peer(PeerId::user_unchecked(7))
            .await
            .expect("peer")
            .expect("cached");
        assert_eq!(peer.auth(), Some(PeerAuth::from_hash(-1_234)));
        assert_eq!(
            restored.updates_state().await.expect("state"),
            first.data.updates_state
        );
    }

    #[tokio::test]
    async fn snapshot_records_datacenters_it_has_keys_for() {
        let session = MemorySession::default();
        let existing = session.dc_option(2).expect("known dc").expect("present");
        let mut updated = existing;
        updated.auth_key = Some([7u8; 256]);
        session.set_dc_option(&updated).await.expect("set");
        session.set_home_dc_id(2).await.expect("home");

        let mut dc_ids = BTreeSet::new();
        dc_ids.insert(2);
        let envelope = snapshot(&session, &PeerCache::new(), None, &dc_ids)
            .await
            .expect("snapshot");

        let option = envelope
            .data
            .dc_options
            .iter()
            .find(|option| option.id == 2)
            .expect("dc recorded");
        assert_eq!(option.auth_key, Some([7u8; 256]));
        assert!(datacenters(&envelope).contains(&2));
    }

    #[test]
    fn a_newer_or_older_layout_is_rejected() {
        let mut envelope = empty_envelope();
        envelope.v = SESSION_VERSION + 1;
        let bytes = encode(&envelope).expect("encode");

        assert_eq!(decode(&bytes).unwrap_err(), ConnectorError::Unauthorized);
    }

    #[test]
    fn junk_and_truncated_blobs_are_rejected() {
        assert_eq!(decode(b"").unwrap_err(), ConnectorError::Unauthorized);
        assert_eq!(
            decode(b"not json").unwrap_err(),
            ConnectorError::Unauthorized
        );
        assert_eq!(
            decode(b"{\"v\":1}").unwrap_err(),
            ConnectorError::Unauthorized
        );
    }

    #[test]
    fn a_current_blob_decodes() {
        let bytes = encode(&empty_envelope()).expect("encode");

        let envelope = decode(&bytes).expect("decode");

        assert_eq!(envelope.v, SESSION_VERSION);
        assert_eq!(envelope.data.home_dc, 2);
        assert!(envelope.peers.is_empty());
        assert!(envelope.myself.is_none());
    }
}
