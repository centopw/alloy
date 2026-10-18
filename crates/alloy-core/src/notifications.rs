//! Notification settings backed by the Matrix push-rule API.
//!
//! Room-level and default levels map onto the SDK's `RoomNotificationMode`
//! (underride and room rules). Mention and invite flags map onto the
//! predefined override rules.
//!
//! All calls share one `NotificationSettings` per client. Its sync handler keeps
//! its rules current, and each write updates them directly, so a read that
//! follows a write sees the new value.

use alloy_protocol::{NotificationLevel, NotificationSettingsDto, RoomId};
use matrix_sdk::notification_settings::{
    IsEncrypted, IsOneToOne, NotificationSettings, RoomNotificationMode,
};
use matrix_sdk::ruma::push::{PredefinedOverrideRuleId, RuleKind};

use crate::{AlloyClient, AlloyError, AlloyResult};

/// Effective level of one room, plus whether it comes from the default rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomNotificationState {
    pub level: NotificationLevel,
    pub is_default: bool,
}

fn sdk_error(error: impl std::fmt::Display) -> AlloyError {
    AlloyError::MatrixSdk(error.to_string())
}

fn level_from_mode(mode: RoomNotificationMode) -> NotificationLevel {
    match mode {
        RoomNotificationMode::AllMessages => NotificationLevel::All,
        RoomNotificationMode::MentionsAndKeywordsOnly => NotificationLevel::Mentions,
        RoomNotificationMode::Mute => NotificationLevel::Mute,
    }
}

fn mode_from_level(level: NotificationLevel) -> RoomNotificationMode {
    match level {
        NotificationLevel::All => RoomNotificationMode::AllMessages,
        NotificationLevel::Mentions => RoomNotificationMode::MentionsAndKeywordsOnly,
        NotificationLevel::Mute => RoomNotificationMode::Mute,
    }
}

/// Default levels come from underride rules, which cannot express a muted
/// default. Mute is rejected instead of being silently stored as Mentions.
fn default_mode_from_level(level: NotificationLevel) -> AlloyResult<RoomNotificationMode> {
    match level {
        NotificationLevel::All => Ok(RoomNotificationMode::AllMessages),
        NotificationLevel::Mentions => Ok(RoomNotificationMode::MentionsAndKeywordsOnly),
        NotificationLevel::Mute => Err(sdk_error(
            "a default notification level cannot be Mute; use All or Mentions",
        )),
    }
}

async fn override_rule_enabled(
    settings: &NotificationSettings,
    rule: PredefinedOverrideRuleId,
) -> AlloyResult<bool> {
    settings
        .is_push_rule_enabled(RuleKind::Override, rule.as_str())
        .await
        .map_err(sdk_error)
}

impl AlloyClient {
    /// The notification settings shared by every call on this client. Created on
    /// first use and kept alive so its sync handler keeps the rules current.
    async fn shared_notification_settings(&self) -> &NotificationSettings {
        self.notification_settings
            .get_or_init(|| self.client.notification_settings())
            .await
    }

    /// Account-wide default levels and mention/invite rules.
    pub async fn get_notification_settings(&self) -> AlloyResult<NotificationSettingsDto> {
        let settings = self.shared_notification_settings().await;
        let one_to_one = settings
            .get_default_room_notification_mode(IsEncrypted::No, IsOneToOne::Yes)
            .await;
        let group = settings
            .get_default_room_notification_mode(IsEncrypted::No, IsOneToOne::No)
            .await;
        Ok(NotificationSettingsDto {
            one_to_one: level_from_mode(one_to_one),
            group: level_from_mode(group),
            room_mention: override_rule_enabled(settings, PredefinedOverrideRuleId::IsRoomMention)
                .await?,
            user_mention: override_rule_enabled(settings, PredefinedOverrideRuleId::IsUserMention)
                .await?,
            invite: override_rule_enabled(settings, PredefinedOverrideRuleId::InviteForMe).await?,
        })
    }

    /// Sets the default level for one-to-one or group rooms.
    ///
    /// Both the encrypted and unencrypted defaults are written so the level
    /// applies regardless of room encryption.
    pub async fn set_default_notification_level(
        &self,
        one_to_one: bool,
        level: NotificationLevel,
    ) -> AlloyResult<()> {
        let mode = default_mode_from_level(level)?;
        let settings = self.shared_notification_settings().await;
        let is_one_to_one = IsOneToOne::from(one_to_one);
        for is_encrypted in [IsEncrypted::Yes, IsEncrypted::No] {
            settings
                .set_default_room_notification_mode(is_encrypted, is_one_to_one, mode)
                .await
                .map_err(sdk_error)?;
        }
        Ok(())
    }

    /// Enables or disables the mention and invite override rules that are `Some`.
    pub async fn set_mention_rules(
        &self,
        room_mention: Option<bool>,
        user_mention: Option<bool>,
        invite: Option<bool>,
    ) -> AlloyResult<()> {
        let updates = [
            (PredefinedOverrideRuleId::IsRoomMention, room_mention),
            (PredefinedOverrideRuleId::IsUserMention, user_mention),
            (PredefinedOverrideRuleId::InviteForMe, invite),
        ];
        let settings = self.shared_notification_settings().await;
        for (rule, enabled) in updates {
            if let Some(enabled) = enabled {
                settings
                    .set_push_rule_enabled(RuleKind::Override, rule.as_str(), enabled)
                    .await
                    .map_err(sdk_error)?;
            }
        }
        Ok(())
    }

    /// Effective level for one room. Falls back to the default for the room's
    /// encryption and one-to-one state when no room rule exists.
    pub async fn get_room_notification_level(
        &self,
        room_id: &RoomId,
    ) -> AlloyResult<RoomNotificationState> {
        let room = self.room(room_id)?;
        let settings = self.shared_notification_settings().await;
        let user_defined = settings
            .get_user_defined_room_notification_mode(room.room_id())
            .await;
        let mode = match user_defined {
            Some(mode) => mode,
            None => {
                let is_encrypted = IsEncrypted::from(room.encryption_state().is_encrypted());
                let is_one_to_one = IsOneToOne::from(room.active_members_count() == 2);
                settings
                    .get_default_room_notification_mode(is_encrypted, is_one_to_one)
                    .await
            }
        };
        Ok(RoomNotificationState {
            level: level_from_mode(mode),
            is_default: user_defined.is_none(),
        })
    }

    /// Sets a room-level rule, or removes it when `level` is `None`.
    pub async fn set_room_notification_level(
        &self,
        room_id: &RoomId,
        level: Option<NotificationLevel>,
    ) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        let settings = self.shared_notification_settings().await;
        match level.map(mode_from_level) {
            Some(mode) => settings
                .set_room_notification_mode(room.room_id(), mode)
                .await
                .map_err(sdk_error),
            None => settings
                .delete_user_defined_room_rules(room.room_id())
                .await
                .map_err(sdk_error),
        }
    }
}
