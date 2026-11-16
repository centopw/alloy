//! Versioned configuration bundle: export and import of the client-owned configuration layers.
//!
//! A bundle is the enrolment payload (ADR 0020, slice A). It carries settings, chat tags, the
//! catalogue, user backgrounds and the theme selection, and nothing else: accounts, messages,
//! participants, outbox entries and connector session material cannot be represented, and
//! [`ConfigBundle`] rejects unknown top-level fields so a hand-written bundle cannot smuggle them
//! in.
//!
//! Merge rules:
//! - The bundle schema is exact. A bundle from another schema is rejected with an error naming
//!   both versions; nothing is downgraded or ignored.
//! - Tags, backgrounds and the theme are last-write-wins per entity, by `updated_at_ms`. A
//!   `deleted` entity is a tombstone: it is kept after import, so a later bundle carrying the old
//!   value cannot resurrect it.
//! - Settings and the catalogue are whole-layer values stamped by `exported_at_ms`, the moment
//!   the exporting device snapshotted them. A `null` layer means "this device has none" and is
//!   never imported, because an empty device must not wipe a configured one.
//! - Every rule is strictly newer, so importing the same bundle twice applies nothing and leaves
//!   the state byte-identical.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The only bundle schema this build reads and writes. Bumped when the wire shape changes.
pub const BUNDLE_SCHEMA: u32 = 1;

/// Largest single background file accepted from a bundle.
pub const MAX_ASSET_BYTES: usize = 4 * 1024 * 1024;

/// Largest total number of background bytes accepted from a bundle.
pub const MAX_BUNDLE_BYTES: usize = 8 * 1024 * 1024;

/// Largest serialized bundle. A `Vec<u8>` becomes a JSON array of numbers, so the wire form is up
/// to four times the raw bytes; this caps what a receiving device must buffer before parsing.
pub const MAX_BUNDLE_JSON_BYTES: usize = 48 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum BundleError {
    #[error(
        "bundle schema {found} is not supported; this build reads and writes schema {supported}"
    )]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("bundle is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("background {0:?} is not a safe relative path")]
    UnsafeBackgroundPath(String),
    #[error("background {path:?} is {size} bytes, over the {limit}-byte per-file limit")]
    BackgroundTooLarge {
        path: String,
        size: usize,
        limit: usize,
    },
    #[error("bundle carries {size} background bytes, over the {limit}-byte limit")]
    BundleTooLarge { size: usize, limit: usize },
    #[error("serialized bundle is {size} bytes, over the {limit}-byte limit")]
    SerializedTooLarge { size: usize, limit: usize },
    #[error("tag {0:?} is empty or longer than 64 characters")]
    InvalidTag(String),
    #[error("theme selection has an empty id")]
    EmptyThemeId,
}

/// Stable identity of a chat across devices. Both parts come from the connector, not from a local
/// database row: the account label and the network's own room id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatKey {
    pub account: String,
    pub room: String,
}

impl ChatKey {
    pub fn new(account: impl Into<String>, room: impl Into<String>) -> Self {
        let account = account.into();
        let room = room.into();
        Self { account, room }
    }
}

/// One chat's tag set, with the stamp that orders it against other devices' copies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagSet {
    pub updated_at_ms: i64,
    /// Tombstone: the tags were cleared on some device after the tags listed here were set.
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub tags: BTreeSet<String>,
}

impl TagSet {
    pub fn new(tags: impl IntoIterator<Item = String>, updated_at_ms: i64) -> Self {
        Self {
            updated_at_ms,
            deleted: false,
            tags: tags.into_iter().collect(),
        }
    }

    /// A deletion. Tags are dropped so the bundle does not keep what the tombstone replaces.
    pub fn deleted(updated_at_ms: i64) -> Self {
        Self {
            updated_at_ms,
            deleted: true,
            tags: BTreeSet::new(),
        }
    }
}

/// A user background file, carried whole so the receiving device needs no file sharing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundAsset {
    /// Path relative to the app's background directory, with `/` separators.
    pub relative_path: String,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub bytes: Vec<u8>,
}

/// The selected theme. `theme` is present only for a user theme; a built-in theme is just its id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSelection {
    pub id: String,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<serde_json::Value>,
}

/// One device's configuration, as transferred during enrolment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigBundle {
    pub schema: u32,
    pub exported_at_ms: i64,
    pub device_label: String,
    pub settings: serde_json::Value,
    /// JSON has string-only object keys, so the map travels as an array of key/value pairs.
    #[serde(with = "chat_tag_map")]
    pub tags: BTreeMap<ChatKey, TagSet>,
    pub catalogue: serde_json::Value,
    pub backgrounds: Vec<BackgroundAsset>,
    pub theme: Option<ThemeSelection>,
}

impl ConfigBundle {
    /// Parse and validate. Unknown top-level fields and unknown schemas are errors, so a bundle
    /// that a future version wrote is refused instead of being read as a partial bundle.
    pub fn from_json(bytes: &[u8]) -> Result<Self, BundleError> {
        let bundle: Self = serde_json::from_slice(bytes)?;
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, BundleError> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_BUNDLE_JSON_BYTES {
            return Err(BundleError::SerializedTooLarge {
                size: bytes.len(),
                limit: MAX_BUNDLE_JSON_BYTES,
            });
        }
        Ok(bytes)
    }

    pub fn validate(&self) -> Result<(), BundleError> {
        if self.schema != BUNDLE_SCHEMA {
            return Err(BundleError::UnsupportedSchema {
                found: self.schema,
                supported: BUNDLE_SCHEMA,
            });
        }
        let mut total = 0usize;
        for asset in &self.backgrounds {
            if !is_safe_relative_path(&asset.relative_path) {
                return Err(BundleError::UnsafeBackgroundPath(
                    asset.relative_path.clone(),
                ));
            }
            if asset.bytes.len() > MAX_ASSET_BYTES {
                return Err(BundleError::BackgroundTooLarge {
                    path: asset.relative_path.clone(),
                    size: asset.bytes.len(),
                    limit: MAX_ASSET_BYTES,
                });
            }
            total = total.saturating_add(asset.bytes.len());
        }
        if total > MAX_BUNDLE_BYTES {
            return Err(BundleError::BundleTooLarge {
                size: total,
                limit: MAX_BUNDLE_BYTES,
            });
        }
        for set in self.tags.values() {
            for tag in &set.tags {
                if tag.trim().is_empty() || tag.chars().count() > 64 {
                    return Err(BundleError::InvalidTag(tag.clone()));
                }
            }
        }
        if let Some(theme) = &self.theme
            && theme.id.trim().is_empty()
        {
            return Err(BundleError::EmptyThemeId);
        }
        Ok(())
    }

    /// Merge this bundle into `state`. Validation runs before anything is written, so a rejected
    /// bundle leaves the state untouched.
    pub fn import_into(&self, state: &mut ConfigState) -> Result<ImportReport, BundleError> {
        self.validate()?;
        let mut report = ImportReport::default();

        if !self.settings.is_null() && self.exported_at_ms > state.settings_updated_at_ms {
            state.settings = self.settings.clone();
            state.settings_updated_at_ms = self.exported_at_ms;
            report.settings = true;
        }
        if !self.catalogue.is_null() && self.exported_at_ms > state.catalogue_updated_at_ms {
            state.catalogue = self.catalogue.clone();
            state.catalogue_updated_at_ms = self.exported_at_ms;
            report.catalogue = true;
        }

        for (key, incoming) in &self.tags {
            let apply = state
                .tags
                .get(key)
                .is_none_or(|current| incoming.updated_at_ms > current.updated_at_ms);
            if apply {
                state.tags.insert(key.clone(), incoming.clone());
                report.tags += 1;
            }
        }

        for incoming in &self.backgrounds {
            let apply = state
                .backgrounds
                .get(&incoming.relative_path)
                .is_none_or(|current| incoming.updated_at_ms > current.updated_at_ms);
            if apply {
                state
                    .backgrounds
                    .insert(incoming.relative_path.clone(), incoming.clone());
                report.backgrounds += 1;
            }
        }

        if let Some(incoming) = &self.theme {
            let apply = state
                .theme
                .as_ref()
                .is_none_or(|current| incoming.updated_at_ms > current.updated_at_ms);
            if apply {
                state.theme = Some(incoming.clone());
                report.theme = true;
            }
        }

        Ok(report)
    }
}

/// What an import changed. An all-false report means the bundle was already covered by the local
/// state, which is what makes a second import of the same bundle a no-op.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub settings: bool,
    pub catalogue: bool,
    pub tags: usize,
    pub backgrounds: usize,
    pub theme: bool,
}

impl ImportReport {
    pub fn is_empty(&self) -> bool {
        !self.settings && !self.catalogue && self.tags == 0 && self.backgrounds == 0 && !self.theme
    }
}

/// The local side of an exchange. Serializable as-is, because the example and the settings UI both
/// keep it in a JSON file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigState {
    pub settings: serde_json::Value,
    pub catalogue: serde_json::Value,
    #[serde(with = "chat_tag_map")]
    pub tags: BTreeMap<ChatKey, TagSet>,
    pub backgrounds: BTreeMap<String, BackgroundAsset>,
    pub theme: Option<ThemeSelection>,
    #[serde(default)]
    pub settings_updated_at_ms: i64,
    #[serde(default)]
    pub catalogue_updated_at_ms: i64,
}

impl ConfigState {
    /// Snapshot for another device. `exported_at_ms` is the stamp of the settings and catalogue
    /// values, and the export is taken at `now_ms`.
    pub fn export(&self, device_label: impl Into<String>, now_ms: i64) -> ConfigBundle {
        ConfigBundle {
            schema: BUNDLE_SCHEMA,
            exported_at_ms: now_ms,
            device_label: device_label.into(),
            settings: self.settings.clone(),
            tags: self.tags.clone(),
            catalogue: self.catalogue.clone(),
            backgrounds: self.backgrounds.values().cloned().collect(),
            theme: self.theme.clone(),
        }
    }
}

/// `BTreeMap<ChatKey, TagSet>` as JSON: keys are structs, so the map is written as pairs.
mod chat_tag_map {
    use std::collections::BTreeMap;

    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{ChatKey, TagSet};

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<ChatKey, TagSet>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        map.iter().collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<ChatKey, TagSet>, D::Error> {
        let pairs = Vec::<(ChatKey, TagSet)>::deserialize(deserializer)?;
        let mut map = BTreeMap::new();
        for (key, value) in pairs {
            if map.insert(key, value).is_some() {
                return Err(D::Error::custom("duplicate chat key in tags"));
            }
        }
        Ok(map)
    }
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chat() -> ChatKey {
        ChatKey::new("tg:alice", "room-42")
    }

    fn sample_state() -> ConfigState {
        let mut state = ConfigState {
            settings: json!({"send_on_enter": true, "density": "comfortable"}),
            settings_updated_at_ms: 1_000,
            catalogue: json!({"categories": ["Work", "Family"]}),
            catalogue_updated_at_ms: 1_000,
            theme: Some(ThemeSelection {
                id: "midnight".into(),
                updated_at_ms: 1_100,
                deleted: false,
                theme: Some(json!({"accent": "#7aa2f7"})),
            }),
            ..ConfigState::default()
        };
        state.tags.insert(
            chat(),
            TagSet::new(["work".to_string(), "urgent".to_string()], 1_200),
        );
        state.backgrounds.insert(
            "backgrounds/demo.png".into(),
            BackgroundAsset {
                relative_path: "backgrounds/demo.png".into(),
                updated_at_ms: 1_300,
                deleted: false,
                bytes: b"not really a png".to_vec(),
            },
        );
        state
    }

    #[test]
    fn roundtrip_through_json_preserves_every_layer() {
        let state = sample_state();
        let bundle = state.export("desk", 2_000);
        let bytes = bundle.to_json().unwrap();
        let parsed = ConfigBundle::from_json(&bytes).unwrap();
        assert_eq!(parsed, bundle);

        let mut imported = ConfigState::default();
        let report = parsed.import_into(&mut imported).unwrap();
        assert_eq!(
            report,
            ImportReport {
                settings: true,
                catalogue: true,
                tags: 1,
                backgrounds: 1,
                theme: true,
            }
        );
        assert_eq!(imported.settings, state.settings);
        assert_eq!(imported.tags, state.tags);
        assert_eq!(imported.backgrounds, state.backgrounds);
        assert_eq!(imported.theme, state.theme);
    }

    #[test]
    fn schema_mismatch_names_both_versions() {
        let mut bundle = sample_state().export("desk", 2_000);
        bundle.schema = BUNDLE_SCHEMA + 1;
        let err = bundle.import_into(&mut ConfigState::default()).unwrap_err();
        let message = err.to_string();
        assert!(matches!(
            err,
            BundleError::UnsupportedSchema {
                found,
                supported: BUNDLE_SCHEMA
            } if found == BUNDLE_SCHEMA + 1
        ));
        assert!(message.contains(&(BUNDLE_SCHEMA + 1).to_string()));
        assert!(message.contains(&BUNDLE_SCHEMA.to_string()));

        // The same refusal happens on the way in from JSON, before a caller can read the bundle.
        let mut json = serde_json::to_value(sample_state().export("desk", 2_000)).unwrap();
        json["schema"] = json!(0);
        let err = ConfigBundle::from_json(&serde_json::to_vec(&json).unwrap()).unwrap_err();
        assert!(matches!(
            err,
            BundleError::UnsupportedSchema { found: 0, .. }
        ));
    }

    #[test]
    fn import_is_strictly_newer_and_tombstones_survive() {
        let mut state = ConfigState::default();
        let mut older = ConfigState::default();
        older.tags.insert(chat(), TagSet::new(["work".into()], 100));
        let report = older.export("old", 100).import_into(&mut state).unwrap();
        assert_eq!(report.tags, 1);
        assert_eq!(state.tags[&chat()].tags, BTreeSet::from(["work".into()]));

        // A newer deletion wins and is stored as a tombstone.
        let mut deleting = ConfigState::default();
        deleting.tags.insert(chat(), TagSet::deleted(200));
        assert_eq!(
            deleting
                .export("new", 200)
                .import_into(&mut state)
                .unwrap()
                .tags,
            1
        );
        assert!(state.tags[&chat()].deleted);
        assert!(state.tags[&chat()].tags.is_empty());

        // Re-importing the older bundle cannot resurrect the tags.
        assert!(
            older
                .export("old", 100)
                .import_into(&mut state)
                .unwrap()
                .is_empty()
        );
        assert!(state.tags[&chat()].deleted);

        // Same stamp, same content: not applied, because newer must be strict.
        assert!(
            deleting
                .export("new", 200)
                .import_into(&mut state)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn double_import_is_a_no_op() {
        let bundle = sample_state().export("desk", 2_000);
        let mut state = ConfigState::default();
        assert!(!bundle.import_into(&mut state).unwrap().is_empty());
        let after_first = state.clone();
        let second = bundle.import_into(&mut state).unwrap();
        assert!(second.is_empty());
        assert_eq!(state, after_first);
    }

    #[test]
    fn settings_from_a_newer_bundle_win_and_null_never_wipes() {
        let mut state = ConfigState::default();
        let newer = ConfigState {
            settings: json!({"density": "compact"}),
            settings_updated_at_ms: 500,
            ..ConfigState::default()
        };
        newer.export("a", 500).import_into(&mut state).unwrap();
        assert_eq!(state.settings, json!({"density": "compact"}));

        let empty = ConfigState::default();
        empty.export("b", 900).import_into(&mut state).unwrap();
        assert_eq!(
            state.settings,
            json!({"density": "compact"}),
            "a device with no settings must not erase another device's settings"
        );
    }

    #[test]
    fn a_bundle_cannot_carry_accounts_or_messages() {
        let mut json = serde_json::to_value(sample_state().export("desk", 2_000)).unwrap();
        json["accounts"] = json!([{"id": "tg:alice"}]);
        let err = ConfigBundle::from_json(&serde_json::to_vec(&json).unwrap()).unwrap_err();
        assert!(matches!(err, BundleError::Json(_)));
        assert!(err.to_string().contains("accounts"));

        let mut json = serde_json::to_value(sample_state().export("desk", 2_000)).unwrap();
        json["messages"] = json!([{"id": 1, "text": "hi"}]);
        assert!(matches!(
            ConfigBundle::from_json(&serde_json::to_vec(&json).unwrap()),
            Err(BundleError::Json(_))
        ));
    }

    #[test]
    fn unsafe_paths_and_oversized_assets_are_rejected_before_any_write() {
        for path in ["/etc/passwd", "../escape.png", "a/../b.png", "", "C:evil"] {
            let mut bundle = ConfigState::default().export("desk", 1);
            bundle.backgrounds.push(BackgroundAsset {
                relative_path: path.into(),
                updated_at_ms: 10,
                deleted: false,
                bytes: Vec::new(),
            });
            let mut state = ConfigState::default();
            let err = bundle.import_into(&mut state).unwrap_err();
            assert!(
                matches!(err, BundleError::UnsafeBackgroundPath(_)),
                "{path:?} produced {err:?}"
            );
            assert_eq!(state, ConfigState::default());
        }

        let mut bundle = ConfigState::default().export("desk", 1);
        bundle.backgrounds.push(BackgroundAsset {
            relative_path: "big.bin".into(),
            updated_at_ms: 10,
            deleted: false,
            bytes: vec![0; MAX_ASSET_BYTES + 1],
        });
        assert!(matches!(
            bundle.import_into(&mut ConfigState::default()),
            Err(BundleError::BackgroundTooLarge { .. })
        ));
    }
}
