//! User-defined room tags. The account's tag catalogue (display order, names, colours) lives
//! in global account data; each room's assignment is a standard Matrix room tag `u.<id>`.

use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{RoomId, RoomTagDef, TagColour};
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
    ///
    /// Falls back to the sync-populated local store when the server cannot be reached, so a
    /// restart without a connection still has the account's tags. When the server is unreachable
    /// and nothing is cached, the server error is returned rather than an empty catalogue.
    pub async fn get_tag_catalog(&self) -> AlloyResult<Vec<RoomTagDef>> {
        let account = self.client.account();
        let stored = match account
            .fetch_account_data(GlobalAccountDataEventType::from(TAG_CATALOG_EVENT_TYPE))
            .await
        {
            Ok(stored) => stored,
            Err(server_error) => {
                match account
                    .account_data_raw(GlobalAccountDataEventType::from(TAG_CATALOG_EVENT_TYPE))
                    .await
                {
                    Ok(Some(cached)) => Some(cached),
                    _ => return Err(AlloyError::MatrixSdk(server_error.to_string())),
                }
            }
        };
        catalog_from_stored(stored)
    }

    /// Replaces the room tag catalogue. Entries are validated and trimmed before storing.
    ///
    /// Kept for compatibility while the UI moves to the per-operation commands; those apply one
    /// change to the catalogue the server holds, so a stale local copy cannot drop another
    /// client's entry.
    pub async fn save_tag_catalog(&self, tags: Vec<RoomTagDef>) -> AlloyResult<()> {
        let tags = normalize_tag_catalog(tags)?;
        self.write_tag_catalog(&tags).await
    }

    /// Adds one tag to the catalogue, assigning its id and returning the stored definition. The
    /// catalogue is read from the server first, so an entry another client added survives.
    pub async fn create_room_tag(&self, name: &str, colour: TagColour) -> AlloyResult<RoomTagDef> {
        let name = normalize_tag_name(name)?;
        let mut catalogue = self.get_tag_catalog().await?;
        let tag = RoomTagDef {
            id: unique_tag_id(&name, &catalogue),
            name,
            colour,
        };
        catalogue.push(tag.clone());
        self.write_tag_catalog(&catalogue).await?;
        Ok(tag)
    }

    /// Renames and/or recolours one tag. At least one of `name`/`colour` must be given; the id is
    /// stable, so room assignments are untouched. An id the catalogue does not hold is an error.
    pub async fn rename_room_tag(
        &self,
        id: &str,
        name: Option<&str>,
        colour: Option<TagColour>,
    ) -> AlloyResult<()> {
        if name.is_none() && colour.is_none() {
            return Err(AlloyError::InvalidRoomTag(
                "a rename needs a new name, a new colour, or both".into(),
            ));
        }
        validate_tag_id(id)?;
        let name = name.map(normalize_tag_name).transpose()?;
        let mut catalogue = self.get_tag_catalog().await?;
        let tag = catalogue
            .iter_mut()
            .find(|tag| tag.id == id)
            .ok_or_else(|| AlloyError::RoomTagNotFound(id.to_owned()))?;
        if let Some(name) = name {
            tag.name = name;
        }
        if let Some(colour) = colour {
            tag.colour = colour;
        }
        self.write_tag_catalog(&catalogue).await
    }

    /// Sets the catalogue's display order. `ids` must be a permutation of the ids the catalogue
    /// currently holds: every id exactly once. A list built from a stale copy is refused instead of
    /// silently dropping the entries it does not know about.
    pub async fn reorder_room_tags(&self, ids: &[String]) -> AlloyResult<()> {
        let mut catalogue = self.get_tag_catalog().await?;
        let mut ordered = Vec::with_capacity(ids.len());
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id.as_str()) {
                return Err(AlloyError::InvalidRoomTag(format!(
                    "the new order names {id:?} more than once"
                )));
            }
            let index = catalogue
                .iter()
                .position(|tag| tag.id == *id)
                .ok_or_else(|| {
                    AlloyError::InvalidRoomTag(format!(
                        "the new order names {id:?}, which is not in the catalogue"
                    ))
                })?;
            ordered.push(catalogue.remove(index));
        }
        if let Some(left) = catalogue.first() {
            return Err(AlloyError::InvalidRoomTag(format!(
                "the new order leaves out {:?}; it must list every tag exactly once",
                left.id
            )));
        }
        self.write_tag_catalog(&ordered).await
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

/// Parses a stored catalogue event; a missing event is an empty catalogue.
fn catalog_from_stored(
    stored: Option<Raw<AnyGlobalAccountDataEventContent>>,
) -> AlloyResult<Vec<RoomTagDef>> {
    match stored {
        None => Ok(Vec::new()),
        Some(raw) => {
            let content = raw
                .cast_unchecked::<TagCatalogContent>()
                .deserialize()
                .map_err(|e| {
                    AlloyError::InvalidRoomTag(format!("stored tag catalogue is malformed: {e}"))
                })?;
            Ok(content.tags)
        }
    }
}

/// Trimmed, non-empty name of at most [`MAX_TAG_LEN`] characters.
fn normalize_tag_name(name: &str) -> AlloyResult<String> {
    let name = name.trim();
    if !(1..=MAX_TAG_LEN).contains(&name.chars().count()) {
        return Err(AlloyError::InvalidRoomTag(format!(
            "tag name must be 1-{MAX_TAG_LEN} characters: {name:?}"
        )));
    }
    Ok(name.to_owned())
}

/// A tag id derived from the name: lowercase ASCII alphanumerics, single `-` between words.
fn tag_slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug = &slug[..slug.len().min(MAX_TAG_LEN)];
    if slug.is_empty() {
        "tag".to_owned()
    } else {
        slug.to_owned()
    }
}

/// The slug of `name`, or the first `slug-2`, `slug-3`, … the catalogue does not hold yet.
fn unique_tag_id(name: &str, catalogue: &[RoomTagDef]) -> String {
    let base = tag_slug(name);
    if !catalogue.iter().any(|tag| tag.id == base) {
        return base;
    }
    for n in 2..10_000u32 {
        let suffix = format!("-{n}");
        let stem = &base[..base.len().min(MAX_TAG_LEN - suffix.len())];
        let candidate = format!("{stem}{suffix}");
        if !catalogue.iter().any(|tag| tag.id == candidate) {
            return candidate;
        }
    }
    format!("{}-overflow", &base[..base.len().min(MAX_TAG_LEN - 9)])
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

    #[test]
    fn created_ids_are_slugs_and_stay_unique() {
        assert_eq!(tag_slug("Work"), "work");
        assert_eq!(tag_slug("  Work & Travel  "), "work-travel");
        assert_eq!(tag_slug("!!!"), "tag");
        assert_eq!(tag_slug(&"a".repeat(40)).len(), MAX_TAG_LEN);

        let def = |id: &str| RoomTagDef {
            id: id.into(),
            name: id.into(),
            colour: TagColour::Blue,
        };
        let existing = vec![def("work"), def("work-2")];
        assert_eq!(unique_tag_id("Work", &existing), "work-3");
        assert_eq!(unique_tag_id("Home", &existing), "home");

        let long = unique_tag_id(&"a".repeat(MAX_TAG_LEN), &[def(&"a".repeat(MAX_TAG_LEN))]);
        assert_eq!(long.len(), MAX_TAG_LEN);
        assert!(validate_tag_id(&long).is_ok());
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(normalize_tag_name("  Work  ").unwrap(), "Work");
        assert!(normalize_tag_name("   ").is_err());
        assert!(normalize_tag_name(&"x".repeat(33)).is_err());
        assert!(normalize_tag_name(&"x".repeat(32)).is_ok());
    }

    #[test]
    fn stored_catalogues_parse_and_malformed_ones_are_reported() {
        assert!(catalog_from_stored(None).unwrap().is_empty());

        let raw = Raw::new(&TagCatalogContent {
            tags: vec![tag("work", "Work")],
        })
        .unwrap()
        .cast_unchecked::<AnyGlobalAccountDataEventContent>();
        let parsed = catalog_from_stored(Some(raw)).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "work");
        assert_eq!(parsed[0].name, "Work");

        let malformed = Raw::new(&serde_json::json!({ "tags": "not a list" }))
            .unwrap()
            .cast_unchecked::<AnyGlobalAccountDataEventContent>();
        assert!(matches!(
            catalog_from_stored(Some(malformed)),
            Err(AlloyError::InvalidRoomTag(_))
        ));
    }
}
