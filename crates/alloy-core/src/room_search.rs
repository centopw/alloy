//! Server-side search of a room's text messages.
//!
//! The homeserver search index cannot read encrypted events, so encrypted rooms are never sent
//! to the server: the search reports [`RoomSearchScope::Unavailable`] instead.

use crate::mapping::millis;
use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{EventId, RoomId, RoomSearchHit, RoomSearchScope, UserId};
use matrix_sdk::ruma::api::client::filter::RoomEventFilter;
use matrix_sdk::ruma::api::client::search::search_events::v3::{
    Categories, Criteria, EventContext, OrderBy, Request, Response,
};
use matrix_sdk::ruma::events::room::message::Relation;
use matrix_sdk::ruma::events::{AnyMessageLikeEvent, AnyTimelineEvent, MessageLikeEvent};
use matrix_sdk::ruma::{OwnedRoomId, OwnedUserId, UInt};

/// Smallest page size accepted for `limit`.
const LIMIT_MIN: u32 = 1;
/// Largest page size accepted for `limit`.
const LIMIT_MAX: u32 = 50;

/// Clamps a caller-supplied page size to the supported range.
pub(crate) fn clamp_limit(limit: u32) -> u32 {
    limit.clamp(LIMIT_MIN, LIMIT_MAX)
}

/// Rejects a blank search query. The query itself is sent unchanged.
pub(crate) fn validate_query(query: &str) -> AlloyResult<()> {
    if query.trim().is_empty() {
        return Err(AlloyError::InvalidSearch(
            "search query must not be blank".to_owned(),
        ));
    }
    Ok(())
}

/// Builds the `POST /search` request for room events.
///
/// The event context is empty (no surrounding events) but requests the sender profiles, which
/// supply the display names carried on each hit. Results are ordered newest first.
pub(crate) fn build_request(
    room_id: OwnedRoomId,
    query: &str,
    sender: Option<OwnedUserId>,
    limit: u32,
    next_batch: Option<String>,
) -> Request {
    let mut filter = RoomEventFilter::default();
    filter.rooms = Some(vec![room_id]);
    filter.senders = sender.map(|sender| vec![sender]);
    filter.limit = Some(UInt::from(limit));

    let mut event_context = EventContext::new();
    event_context.before_limit = UInt::from(0u32);
    event_context.after_limit = UInt::from(0u32);
    event_context.include_profile = true;

    let mut criteria = Criteria::new(query.to_owned());
    criteria.filter = filter;
    criteria.order_by = Some(OrderBy::Recent);
    criteria.event_context = event_context;

    let mut categories = Categories::new();
    categories.room_events = Some(criteria);

    let mut request = Request::new(categories);
    request.next_batch = next_batch;
    request
}

/// Maps a search response to text hits plus the pagination token.
///
/// Only original `m.room.message` events with a body become hits. Edits (`m.replace`) are
/// skipped because they repeat the message they replace, and redacted or non-message events
/// are skipped because they carry no text.
pub(crate) fn map_response(
    response: Response,
) -> AlloyResult<(Vec<RoomSearchHit>, Option<String>)> {
    let results = response.search_categories.room_events;
    let mut hits = Vec::with_capacity(results.results.len());

    for result in results.results {
        let Some(raw) = result.result else {
            continue;
        };
        let event = raw
            .deserialize()
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        let AnyTimelineEvent::MessageLike(AnyMessageLikeEvent::RoomMessage(
            MessageLikeEvent::Original(message),
        )) = event
        else {
            continue;
        };
        if matches!(message.content.relates_to, Some(Relation::Replacement(_))) {
            continue;
        }

        let sender_display_name = result
            .context
            .profile_info
            .get(&message.sender)
            .and_then(|profile| profile.displayname.clone());

        hits.push(RoomSearchHit {
            event_id: EventId(message.event_id.to_string()),
            sender: UserId(message.sender.to_string()),
            sender_display_name,
            timestamp_millis: millis(message.origin_server_ts),
            body: message.content.body().to_owned(),
        });
    }

    Ok((hits, results.next_batch))
}

impl AlloyClient {
    /// Searches one room's text messages on the homeserver, newest first.
    ///
    /// `next_batch` is the token returned by the previous page. The returned token is `None`
    /// once no further pages exist. Encrypted rooms are not searched and yield
    /// [`RoomSearchScope::Unavailable`] with no hits.
    pub async fn search_room_messages(
        &self,
        room_id: &RoomId,
        query: &str,
        sender: Option<&UserId>,
        limit: u32,
        next_batch: Option<&str>,
    ) -> AlloyResult<(Vec<RoomSearchHit>, Option<String>, RoomSearchScope)> {
        validate_query(query)?;
        let room = self.room(room_id)?;
        if room.encryption_state().is_encrypted() {
            return Ok((Vec::new(), None, RoomSearchScope::Unavailable));
        }

        let sender = sender
            .map(|sender| {
                matrix_sdk::ruma::UserId::parse(&sender.0)
                    .map_err(|e| AlloyError::InvalidSearch(format!("invalid sender: {e}")))
            })
            .transpose()?;

        let request = build_request(
            room.room_id().to_owned(),
            query,
            sender,
            clamp_limit(limit),
            next_batch.map(str::to_owned),
        );
        let response = self
            .client
            .send(request)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        let (hits, next_batch) = map_response(response)?;
        Ok((hits, next_batch, RoomSearchScope::Server))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matrix_sdk::ruma::api::client::search::search_events::v3::{
        ResultCategories, ResultRoomEvents, SearchResult,
    };
    use matrix_sdk::ruma::events::AnyTimelineEvent;
    use matrix_sdk::ruma::serde::Raw;
    use serde_json::json;

    fn room() -> OwnedRoomId {
        matrix_sdk::ruma::RoomId::parse("!room:example.org").unwrap()
    }

    fn user(id: &str) -> OwnedUserId {
        matrix_sdk::ruma::UserId::parse(id).unwrap()
    }

    fn message_event(
        event_id: &str,
        sender: &str,
        body: &str,
        relates_to: Option<serde_json::Value>,
    ) -> Raw<AnyTimelineEvent> {
        let mut content = json!({ "msgtype": "m.text", "body": body });
        if let Some(relates_to) = relates_to {
            content["m.relates_to"] = relates_to;
        }
        let value = json!({
            "type": "m.room.message",
            "event_id": event_id,
            "room_id": "!room:example.org",
            "sender": sender,
            "origin_server_ts": 1_700_000_000_123u64,
            "content": content,
        });
        serde_json::from_value(value).unwrap()
    }

    fn search_result(
        event: Option<Raw<AnyTimelineEvent>>,
        displayname: Option<&str>,
    ) -> SearchResult {
        let mut result = SearchResult::new();
        result.result = event;
        if let Some(name) = displayname {
            let profile = serde_json::from_value(json!({ "displayname": name })).unwrap();
            result
                .context
                .profile_info
                .insert(user("@alice:example.org"), profile);
        }
        result
    }

    fn response(results: Vec<SearchResult>, next_batch: Option<&str>) -> Response {
        let mut room_events = ResultRoomEvents::new();
        room_events.results = results;
        room_events.next_batch = next_batch.map(str::to_owned);
        let mut categories = ResultCategories::new();
        categories.room_events = room_events;
        Response::new(categories)
    }

    #[test]
    fn blank_query_is_rejected() {
        assert!(matches!(
            validate_query(""),
            Err(AlloyError::InvalidSearch(_))
        ));
        assert!(matches!(
            validate_query("   \t"),
            Err(AlloyError::InvalidSearch(_))
        ));
        assert!(validate_query("needle").is_ok());
    }

    #[test]
    fn limit_is_clamped_to_supported_range() {
        assert_eq!(clamp_limit(0), 1);
        assert_eq!(clamp_limit(1), 1);
        assert_eq!(clamp_limit(20), 20);
        assert_eq!(clamp_limit(50), 50);
        assert_eq!(clamp_limit(500), 50);
    }

    #[test]
    fn request_carries_filter_order_and_pagination() {
        let request = build_request(
            room(),
            "needle",
            Some(user("@bob:example.org")),
            7,
            Some("token-1".to_owned()),
        );
        assert_eq!(request.next_batch.as_deref(), Some("token-1"));
        let criteria = request
            .search_categories
            .room_events
            .expect("room_events criteria");
        assert_eq!(criteria.search_term, "needle");
        assert_eq!(criteria.order_by, Some(OrderBy::Recent));
        assert_eq!(criteria.filter.rooms, Some(vec![room()]));
        assert_eq!(
            criteria.filter.senders,
            Some(vec![user("@bob:example.org")])
        );
        assert_eq!(criteria.filter.limit, Some(UInt::from(7u32)));
        assert_eq!(criteria.event_context.before_limit, UInt::from(0u32));
        assert_eq!(criteria.event_context.after_limit, UInt::from(0u32));
        assert!(criteria.event_context.include_profile);
    }

    #[test]
    fn request_without_sender_filters_no_senders() {
        let request = build_request(room(), "haystack", None, 50, None);
        assert!(request.next_batch.is_none());
        let criteria = request
            .search_categories
            .room_events
            .expect("room_events criteria");
        assert!(criteria.filter.senders.is_none());
    }

    #[test]
    fn maps_text_messages_with_display_name_and_token() {
        let results = vec![search_result(
            Some(message_event(
                "$e1:example.org",
                "@alice:example.org",
                "needle alpha",
                None,
            )),
            Some("Alice"),
        )];
        let (hits, next_batch) = map_response(response(results, Some("next-page"))).unwrap();
        assert_eq!(next_batch.as_deref(), Some("next-page"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].event_id, EventId("$e1:example.org".to_owned()));
        assert_eq!(hits[0].sender, UserId("@alice:example.org".to_owned()));
        assert_eq!(hits[0].sender_display_name.as_deref(), Some("Alice"));
        assert_eq!(hits[0].timestamp_millis, 1_700_000_000_123);
        assert_eq!(hits[0].body, "needle alpha");
    }

    #[test]
    fn missing_profile_yields_no_display_name() {
        let results = vec![search_result(
            Some(message_event(
                "$e2:example.org",
                "@alice:example.org",
                "haystack",
                None,
            )),
            None,
        )];
        let (hits, next_batch) = map_response(response(results, None)).unwrap();
        assert!(next_batch.is_none());
        assert_eq!(hits.len(), 1);
        assert!(hits[0].sender_display_name.is_none());
    }

    #[test]
    fn skips_edits_and_redacted_results() {
        let edit: Raw<AnyTimelineEvent> = serde_json::from_value(json!({
            "type": "m.room.message",
            "event_id": "$edit:example.org",
            "room_id": "!room:example.org",
            "sender": "@alice:example.org",
            "origin_server_ts": 1_700_000_000_123u64,
            "content": {
                "msgtype": "m.text",
                "body": "* needle alpha edited",
                "m.new_content": { "msgtype": "m.text", "body": "needle alpha edited" },
                "m.relates_to": { "rel_type": "m.replace", "event_id": "$e1:example.org" },
            },
        }))
        .unwrap();
        let redacted: Raw<AnyTimelineEvent> = serde_json::from_value(json!({
            "type": "m.room.message",
            "event_id": "$gone:example.org",
            "room_id": "!room:example.org",
            "sender": "@alice:example.org",
            "origin_server_ts": 1u64,
            "content": {},
            "unsigned": {
                "redacted_because": {
                    "type": "m.room.redaction",
                    "event_id": "$redaction:example.org",
                    "room_id": "!room:example.org",
                    "sender": "@alice:example.org",
                    "origin_server_ts": 2u64,
                    "redacts": "$gone:example.org",
                    "content": {},
                },
            },
        }))
        .unwrap();
        let results = vec![
            search_result(Some(edit), None),
            search_result(Some(redacted), None),
            search_result(None, None),
        ];
        let (hits, _) = map_response(response(results, None)).unwrap();
        assert!(hits.is_empty());
    }
}
