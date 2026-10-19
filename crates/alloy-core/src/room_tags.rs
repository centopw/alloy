//! User-defined room tags. The account's tag catalogue (display order, names, colours) lives
//! in global account data; each room's assignment is a standard Matrix room tag `u.<id>`.

use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{RoomId, RoomTagDef};
use matrix_sdk::ruma::events::tag::{TagInfo, TagName, UserTagName};
use matrix_sdk::ruma::events::{AnyGlobalAccountDataEventContent, GlobalAccountDataEventType};
use matrix_sdk::ruma::serde::Raw;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;

/// Global account-data type holding the tag catalogue as `{"tags": [...]}`.
const TAG_CATALOG_EVENT_TYPE: &str = "chat.alloy.room_tags";

/// Prefix of the Matrix room-tag names that carry custom tags.
pub(crate) const USER_ROOM_TAG_PREFIX: &str = "u.";

/// Longest tag id and tag name, in characters.
const MAX_TAG_LEN: usize = 32;

#[derive(Serialize, Deserialize)]
struct TagCatalogContent {
    tags: Vec<RoomTagDef>,
}

impl AlloyClient {
    /// The account's room tag catalogue in display order; empty when none has been saved.
    pub async fn get_tag_catalog(&self) -> AlloyResult<Vec<RoomTagDef>> {
        let stored = self
            .client
            .account()
            .fetch_account_data(GlobalAccountDataEventType::from(TAG_CATALOG_EVENT_TYPE))
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        match stored {
            None => Ok(Vec::new()),
            Some(raw) => {
                let content = raw
                    .cast_unchecked::<TagCatalogContent>()
                    .deserialize()
                    .map_err(|e| {
                        AlloyError::InvalidRoomTag(format!(
                            "stored tag catalogue is malformed: {e}"
                        ))
                    })?;
                Ok(content.tags)
            }
        }
    }

    /// Replaces the room tag catalogue. Entries are validated and trimmed before storing.
    pub async fn save_tag_catalog(&self, tags: Vec<RoomTagDef>) -> AlloyResult<()> {
        let tags = normalize_tag_catalog(tags)?;
        self.write_tag_catalog(&tags).await
    }

    /// Assigns (`enabled`) or removes the custom tag `tag_id` on a room. Idempotent. The tag
    /// need not be in the catalogue, so a tag whose definition was lost still works.
    pub async fn set_room_tag(
        &self,
        room_id: &RoomId,
        tag_id: &str,
        enabled: bool,
    ) -> AlloyResult<()> {
        validate_tag_id(tag_id)?;
        let room = self.room(room_id)?;
        let name = user_tag_name(tag_id)?;
        let result = if enabled {
            room.set_tag(name, TagInfo::default()).await.map(|_| ())
        } else {
            room.remove_tag(name).await.map(|_| ())
        };
        result.map_err(|e| AlloyError::MatrixSdk(e.to_string()))
    }

    /// Deletes a tag: its catalogue entry first, then the `u.<id>` tag on every room carrying it.
    /// Idempotent. If a room update fails the error is returned; the catalogue entry is already
    /// gone, so retrying finishes the removal.
    pub async fn delete_room_tag(&self, tag_id: &str) -> AlloyResult<()> {
        validate_tag_id(tag_id)?;
        let mut catalogue = self.get_tag_catalog().await?;
        let before = catalogue.len();
        catalogue.retain(|tag| tag.id != tag_id);
        if catalogue.len() != before {
            self.write_tag_catalog(&catalogue).await?;
        }

        let name = user_tag_name(tag_id)?;
        for room in self.client.rooms() {
            let tags = room
                .tags()
                .await
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
            if tags.is_some_and(|tags| tags.contains_key(&name)) {
                room.remove_tag(name.clone())
                    .await
                    .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn write_tag_catalog(&self, tags: &[RoomTagDef]) -> AlloyResult<()> {
        let content = Raw::new(&TagCatalogContent {
            tags: tags.to_vec(),
        })
        .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?
        .cast_unchecked::<AnyGlobalAccountDataEventContent>();
        self.client
            .account()
            .set_account_data_raw(
                GlobalAccountDataEventType::from(TAG_CATALOG_EVENT_TYPE),
                content,
            )
            .await
            .map(|_| ())
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))
    }
}

/// A tag id is 1..=32 characters of `[a-z0-9-]`; it is the suffix of the Matrix tag `u.<id>`.
fn validate_tag_id(id: &str) -> AlloyResult<()> {
    let well_formed = (1..=MAX_TAG_LEN).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if well_formed {
        Ok(())
    } else {
        Err(AlloyError::InvalidRoomTag(format!(
            "tag id must be 1-{MAX_TAG_LEN} characters of a-z, 0-9 or '-': {id:?}"
        )))
    }
}

/// Validates a whole catalogue and trims its names. Ids must be well formed and unique.
fn normalize_tag_catalog(tags: Vec<RoomTagDef>) -> AlloyResult<Vec<RoomTagDef>> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .map(|tag| -> AlloyResult<RoomTagDef> {
            validate_tag_id(&tag.id)?;
            let name = tag.name.trim();
            if !(1..=MAX_TAG_LEN).contains(&name.chars().count()) {
                return Err(AlloyError::InvalidRoomTag(format!(
                    "tag name must be 1-{MAX_TAG_LEN} characters: {:?}",
                    tag.name
                )));
            }
            if !seen.insert(tag.id.clone()) {
                return Err(AlloyError::InvalidRoomTag(format!(
                    "duplicate tag id {:?}",
                    tag.id
                )));
            }
            Ok(RoomTagDef {
                name: name.to_owned(),
                ..tag
            })
        })
        .collect()
}

fn user_tag_name(tag_id: &str) -> AlloyResult<TagName> {
    let name = UserTagName::from_str(&format!("{USER_ROOM_TAG_PREFIX}{tag_id}"))
        .map_err(|e| AlloyError::InvalidRoomTag(e.to_string()))?;
    Ok(TagName::User(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_protocol::TagColour;

    fn tag(id: &str, name: &str) -> RoomTagDef {
        RoomTagDef {
            id: id.into(),
            name: name.into(),
            colour: TagColour::Blue,
        }
    }

    #[test]
    fn tag_ids_are_slugs_of_up_to_32_characters() {
        assert!(validate_tag_id("work").is_ok());
        assert!(validate_tag_id("a-1").is_ok());
        assert!(validate_tag_id(&"a".repeat(32)).is_ok());
        assert!(validate_tag_id("").is_err());
        assert!(validate_tag_id(&"a".repeat(33)).is_err());
        assert!(validate_tag_id("Work").is_err());
        assert!(validate_tag_id("two words").is_err());
        assert!(validate_tag_id("café").is_err());
    }

    #[test]
    fn catalogue_names_are_trimmed_and_bounded() {
        let saved = normalize_tag_catalog(vec![tag("work", "  Work  ")]).unwrap();
        assert_eq!(saved[0].name, "Work");
        assert!(normalize_tag_catalog(vec![tag("work", "   ")]).is_err());
        assert!(normalize_tag_catalog(vec![tag("work", &"x".repeat(33))]).is_err());
        assert!(normalize_tag_catalog(vec![tag("work", &"x".repeat(32))]).is_ok());
    }

    #[test]
    fn catalogue_ids_are_unique_and_order_is_kept() {
        assert!(normalize_tag_catalog(vec![tag("work", "Work"), tag("work", "Again")]).is_err());
        let saved = normalize_tag_catalog(vec![tag("b", "B"), tag("a", "A")]).unwrap();
        let ids: Vec<&str> = saved.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
    }

    #[test]
    fn user_tag_names_use_the_u_prefix() {
        assert!(matches!(
            user_tag_name("work").unwrap(),
            TagName::User(ref name) if name.as_ref() == "u.work"
        ));
    }
}
