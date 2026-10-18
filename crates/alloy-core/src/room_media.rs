//! Media, file and link listings for a room.
//!
//! Matrix has no server-side filter that works for encrypted rooms, so the history is scanned
//! page by page (newest first) and filtered client-side. Each call scans at most [`MAX_PAGES`]
//! server pages and returns a cursor when more history remains.

use crate::mapping::{media_source_string, millis};
use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{EventId, RoomId, RoomMediaItem, RoomMediaKind, UserId};
use matrix_sdk::room::MessagesOptions;
use matrix_sdk::ruma::UInt;
use matrix_sdk::ruma::events::room::message::{MessageType, Relation};
use matrix_sdk::ruma::events::{
    AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
};
use std::collections::HashMap;

/// Events requested from the homeserver per page.
const PAGE_SIZE: u32 = 100;
/// Server pages scanned per call before returning a resumable cursor.
const MAX_PAGES: usize = 8;
/// Bounds for the caller-supplied `limit`.
const LIMIT_MIN: u32 = 1;
const LIMIT_MAX: u32 = 100;

impl AlloyClient {
    /// One page of media, files or links from a room, newest first.
    ///
    /// `from` is the cursor returned by the previous call (`None` starts at the newest message).
    /// The returned cursor is `None` once the start of the room history has been reached.
    pub async fn room_media(
        &self,
        room_id: &RoomId,
        kind: RoomMediaKind,
        from: Option<&str>,
        limit: u32,
    ) -> AlloyResult<(Vec<RoomMediaItem>, Option<String>)> {
        let room = self.room(room_id)?;
        let limit = limit.clamp(LIMIT_MIN, LIMIT_MAX) as usize;
        let (mut token, mut skip) = parse_cursor(from);
        let mut items: Vec<RoomMediaItem> = Vec::new();
        // Display name per sender, resolved from the local store only (no network per item).
        let mut names: HashMap<String, Option<String>> = HashMap::new();

        for _ in 0..MAX_PAGES {
            let mut opts = MessagesOptions::backward();
            opts.from = token.clone();
            opts.limit = UInt::try_from(u64::from(PAGE_SIZE)).map_err(sdk_err)?;
            let page = room.messages(opts).await.map_err(sdk_err)?;

            for (idx, event) in page.chunk.iter().enumerate().skip(skip) {
                if items.len() >= limit {
                    return Ok((items, Some(encode_cursor(&page.start, idx))));
                }
                let Ok(any) = event.raw().deserialize() else {
                    continue;
                };
                let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
                    SyncMessageLikeEvent::Original(m),
                )) = any
                else {
                    continue;
                };
                // Edits are shown through the message they replace, not as separate entries.
                if matches!(m.content.relates_to, Some(Relation::Replacement(_))) {
                    continue;
                }
                let Some(described) = describe(&m.content.msgtype, &kind) else {
                    continue;
                };

                let sender = m.sender.to_string();
                let sender_display_name = match names.get(&sender) {
                    Some(name) => name.clone(),
                    None => {
                        let name = room
                            .get_member_no_sync(&m.sender)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|member| member.display_name().map(str::to_owned));
                        names.insert(sender.clone(), name.clone());
                        name
                    }
                };

                items.push(RoomMediaItem {
                    event_id: EventId(m.event_id.to_string()),
                    sender: UserId(sender),
                    sender_display_name,
                    timestamp_millis: millis(m.origin_server_ts),
                    kind: described.kind.to_owned(),
                    body: described.body,
                    uri: described.uri,
                    thumbnail_uri: described.thumbnail_uri,
                    mime_type: described.mime_type,
                    size_bytes: described.size_bytes,
                    width: described.width,
                    height: described.height,
                    urls: described.urls,
                });
            }

            // The whole chunk has been consumed: continue from the server's next token.
            match page.end {
                None => return Ok((items, None)),
                Some(end) => {
                    token = Some(end);
                    skip = 0;
                }
            }
            if items.len() >= limit {
                return Ok((items, token.as_deref().map(|t| encode_cursor(t, 0))));
            }
        }

        Ok((items, token.as_deref().map(|t| encode_cursor(t, 0))))
    }
}

/// Media, file or link fields of one message, before sender information is attached.
struct Described {
    kind: &'static str,
    body: String,
    uri: Option<String>,
    thumbnail_uri: Option<String>,
    mime_type: Option<String>,
    size_bytes: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    urls: Vec<String>,
}

fn described(kind: &'static str, body: String) -> Described {
    Described {
        kind,
        body,
        uri: None,
        thumbnail_uri: None,
        mime_type: None,
        size_bytes: None,
        width: None,
        height: None,
        urls: Vec::new(),
    }
}

/// Classify a message for the requested listing; `None` when it does not belong in it.
fn describe(msgtype: &MessageType, kind: &RoomMediaKind) -> Option<Described> {
    match (kind, msgtype) {
        (RoomMediaKind::Media, MessageType::Image(c)) => {
            let info = c.info.as_deref();
            Some(Described {
                uri: Some(media_source_string(&c.source)),
                thumbnail_uri: info
                    .and_then(|i| i.thumbnail_source.as_ref())
                    .map(media_source_string),
                mime_type: info.and_then(|i| i.mimetype.clone()),
                size_bytes: info.and_then(|i| i.size).map(u64::from),
                width: info.and_then(|i| i.width).and_then(dimension),
                height: info.and_then(|i| i.height).and_then(dimension),
                ..described("image", msgtype.body().to_owned())
            })
        }
        (RoomMediaKind::Media, MessageType::Video(c)) => {
            let info = c.info.as_deref();
            Some(Described {
                uri: Some(media_source_string(&c.source)),
                thumbnail_uri: info
                    .and_then(|i| i.thumbnail_source.as_ref())
                    .map(media_source_string),
                mime_type: info.and_then(|i| i.mimetype.clone()),
                size_bytes: info.and_then(|i| i.size).map(u64::from),
                width: info.and_then(|i| i.width).and_then(dimension),
                height: info.and_then(|i| i.height).and_then(dimension),
                ..described("video", msgtype.body().to_owned())
            })
        }
        (RoomMediaKind::Files, MessageType::File(c)) => {
            let info = c.info.as_deref();
            Some(Described {
                uri: Some(media_source_string(&c.source)),
                thumbnail_uri: info
                    .and_then(|i| i.thumbnail_source.as_ref())
                    .map(media_source_string),
                mime_type: info.and_then(|i| i.mimetype.clone()),
                size_bytes: info.and_then(|i| i.size).map(u64::from),
                ..described("file", c.filename().to_owned())
            })
        }
        (RoomMediaKind::Files, MessageType::Audio(c)) => {
            let info = c.info.as_deref();
            Some(Described {
                uri: Some(media_source_string(&c.source)),
                mime_type: info.and_then(|i| i.mimetype.clone()),
                size_bytes: info.and_then(|i| i.size).map(u64::from),
                ..described("audio", c.filename().to_owned())
            })
        }
        (RoomMediaKind::Links, MessageType::Text(_) | MessageType::Notice(_)) => {
            let body = msgtype.body().to_owned();
            let urls = extract_urls(&body);
            (!urls.is_empty()).then(|| Described {
                urls,
                ..described("link", body)
            })
        }
        _ => None,
    }
}

/// Distinct http(s) URLs in a message body, in order of first appearance.
fn extract_urls(body: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for word in body.split_whitespace() {
        let candidate = word.trim_matches(|c: char| {
            matches!(
                c,
                '(' | ')' | '[' | ']' | '<' | '>' | '"' | '\'' | ',' | '.' | ';' | '!' | '?'
            )
        });
        let Ok(url) = url::Url::parse(candidate) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        if !urls.iter().any(|u| u == candidate) {
            urls.push(candidate.to_owned());
        }
    }
    urls
}

fn dimension(value: UInt) -> Option<u32> {
    u32::try_from(u64::from(value)).ok()
}

/// Cursor layout: `"{server_token}|{events already consumed in that chunk}"`.
/// A cursor without a valid suffix is treated as a bare server token.
fn parse_cursor(from: Option<&str>) -> (Option<String>, usize) {
    let Some(raw) = from.filter(|s| !s.is_empty()) else {
        return (None, 0);
    };
    match raw.rsplit_once('|') {
        Some((token, skip)) if !token.is_empty() => match skip.parse::<usize>() {
            Ok(skip) => (Some(token.to_owned()), skip),
            Err(_) => (Some(raw.to_owned()), 0),
        },
        _ => (Some(raw.to_owned()), 0),
    }
}

fn encode_cursor(token: &str, skip: usize) -> String {
    format!("{token}|{skip}")
}

fn sdk_err(e: impl std::fmt::Display) -> AlloyError {
    AlloyError::MatrixSdk(e.to_string())
}
