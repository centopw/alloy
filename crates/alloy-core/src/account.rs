//! Own profile, the account's device list, and this session's crypto identity.

use std::path::Path;

use alloy_protocol::{DeviceDto, OwnProfileDto, SessionInfoDto, UserId};

use crate::{AlloyClient, AlloyError, AlloyResult};

fn sdk_err(e: impl std::fmt::Display) -> AlloyError {
    AlloyError::MatrixSdk(e.to_string())
}

/// Result of attempting to delete devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteDevicesOutcome {
    Deleted(Vec<String>),
    AuthRequired(Vec<String>),
}

impl AlloyClient {
    /// The logged-in account's profile, including the device this session runs on.
    pub async fn own_profile(&self) -> AlloyResult<OwnProfileDto> {
        let device_id = self
            .client
            .device_id()
            .ok_or(AlloyError::NotLoggedIn)?
            .to_string();
        let profile = self.account_profile().await?;
        Ok(OwnProfileDto {
            user_id: UserId(profile.user_id),
            display_name: profile.display_name,
            avatar_url: profile.avatar_url,
            device_id,
            homeserver: profile.homeserver_url,
        })
    }

    /// Set the account's display name.
    pub async fn set_display_name(&self, name: &str) -> AlloyResult<()> {
        self.client
            .account()
            .set_display_name(Some(name))
            .await
            .map_err(sdk_err)
    }

    /// Upload `file_path` as the account avatar, or clear the avatar when `None`.
    pub async fn set_own_avatar(&self, file_path: Option<&Path>) -> AlloyResult<()> {
        match file_path {
            Some(path) => {
                let mime = crate::media::mime_guess_from_path(path);
                let data = std::fs::read(path)?;
                // `upload_avatar` uploads the media and sets it as the profile avatar.
                self.client
                    .account()
                    .upload_avatar(&mime, data)
                    .await
                    .map_err(sdk_err)?;
                Ok(())
            }
            None => self
                .client
                .account()
                .set_avatar_url(None)
                .await
                .map_err(sdk_err),
        }
    }

    /// Every device registered to the account, this session first, then most recently seen.
    pub async fn list_devices(&self) -> AlloyResult<Vec<DeviceDto>> {
        let own_user = self.client.user_id().ok_or(AlloyError::NotLoggedIn)?;
        let own_device = self.client.device_id().ok_or(AlloyError::NotLoggedIn)?;
        let response = self.client.devices().await.map_err(sdk_err)?;
        let encryption = self.client.encryption();

        let mut devices = Vec::with_capacity(response.devices.len());
        for device in response.devices {
            let is_current = device.device_id.as_str() == own_device.as_str();
            let verified = encryption
                .get_device(own_user, &device.device_id)
                .await
                .map_err(sdk_err)?
                .is_some_and(|known| known.is_verified_with_cross_signing());
            devices.push(DeviceDto {
                device_id: device.device_id.to_string(),
                display_name: device.display_name,
                last_seen_ts: device.last_seen_ts.map(|ts| u64::from(ts.0)),
                last_seen_ip: device.last_seen_ip,
                is_current,
                verified,
            });
        }

        devices.sort_by(|a, b| {
            b.is_current
                .cmp(&a.is_current)
                .then_with(|| b.last_seen_ts.cmp(&a.last_seen_ts))
        });
        Ok(devices)
    }

    /// Crypto identity of this session: device, device keys, cross-signing and key backup state.
    pub async fn session_info(&self) -> AlloyResult<SessionInfoDto> {
        let own_user = self.client.user_id().ok_or(AlloyError::NotLoggedIn)?;
        let own_device = self.client.device_id().ok_or(AlloyError::NotLoggedIn)?;
        let device_id = own_device.to_string();
        let encryption = self.client.encryption();
        let cross_signing_ready = encryption
            .cross_signing_status()
            .await
            .is_some_and(|status| status.is_complete());
        let verified = encryption
            .get_device(own_user, own_device)
            .await
            .map_err(sdk_err)?
            .is_some_and(|known| known.is_verified_with_cross_signing());
        Ok(SessionInfoDto {
            device_id,
            ed25519_key: encryption.ed25519_key().await,
            curve25519_key: encryption.curve25519_key().await.map(|key| key.to_base64()),
            cross_signing_ready,
            backup_enabled: encryption.backups().are_enabled().await,
            verified,
        })
    }

    /// Delete one or more devices from the account. If authentication is required,
    /// returns `DeleteDevicesOutcome::AuthRequired` with the UIAA flows.
    /// Calling this with `password` set will perform the UIAA password challenge.
    pub async fn delete_devices(
        &self,
        device_ids: &[String],
        password: Option<&str>,
    ) -> AlloyResult<DeleteDevicesOutcome> {
        use matrix_sdk::ruma::OwnedDeviceId;
        use matrix_sdk::ruma::api::client::uiaa::{
            AuthData, MatrixUserIdentifier, Password, UserIdentifier,
        };

        let parsed_ids: Vec<OwnedDeviceId> = device_ids
            .iter()
            .map(|id| OwnedDeviceId::from(id.as_str()))
            .collect();

        match self.client.delete_devices(&parsed_ids, None).await {
            Ok(_) => Ok(DeleteDevicesOutcome::Deleted(device_ids.to_vec())),
            Err(e) => {
                let Some(info) = e.as_uiaa_response() else {
                    return Err(sdk_err(e));
                };
                let Some(pwd) = password else {
                    let flows = info
                        .flows
                        .iter()
                        .map(|f| {
                            f.stages
                                .iter()
                                .map(|s| s.as_str().to_string())
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .collect();
                    return Ok(DeleteDevicesOutcome::AuthRequired(flows));
                };
                let user_id = self.client.user_id().ok_or(AlloyError::NotLoggedIn)?;
                let mut pw = Password::new(
                    UserIdentifier::Matrix(MatrixUserIdentifier::new(user_id.to_string())),
                    pwd.to_owned(),
                );
                pw.session = info.session.clone();
                match self
                    .client
                    .delete_devices(&parsed_ids, Some(AuthData::Password(pw)))
                    .await
                {
                    Ok(_) => Ok(DeleteDevicesOutcome::Deleted(device_ids.to_vec())),
                    Err(e) => {
                        if let Some(info) = e.as_uiaa_response()
                            && let Some(auth_err) = &info.auth_error
                        {
                            return Err(AlloyError::Auth(auth_err.message.clone()));
                        }
                        Err(sdk_err(e))
                    }
                }
            }
        }
    }

    /// Add a user to the active account's block list.
    pub async fn ignore_user(&self, user_id: &UserId) -> AlloyResult<()> {
        let parsed_id = matrix_sdk::ruma::UserId::parse(&user_id.0)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        self.client
            .account()
            .ignore_user(&parsed_id)
            .await
            .map_err(sdk_err)
    }

    /// Remove a user from the active account's block list.
    pub async fn unignore_user(&self, user_id: &UserId) -> AlloyResult<()> {
        let parsed_id = matrix_sdk::ruma::UserId::parse(&user_id.0)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        self.client
            .account()
            .unignore_user(&parsed_id)
            .await
            .map_err(sdk_err)
    }

    /// List all users on the active account's block list.
    pub async fn list_ignored_users(&self) -> AlloyResult<Vec<UserId>> {
        use matrix_sdk::ruma::events::GlobalAccountDataEventType;
        use matrix_sdk::ruma::events::ignored_user_list::IgnoredUserListEventContent;

        let content = match self
            .client
            .account()
            .fetch_account_data(GlobalAccountDataEventType::IgnoredUserList)
            .await
        {
            Ok(Some(raw)) => serde_json::from_str::<IgnoredUserListEventContent>(raw.json().get())
                .ok()
                .unwrap_or_default(),
            _ => self
                .client
                .account()
                .account_data::<IgnoredUserListEventContent>()
                .await
                .map_err(sdk_err)?
                .and_then(|raw| raw.deserialize().ok())
                .unwrap_or_default(),
        };

        let mut user_ids: Vec<UserId> = content
            .ignored_users
            .keys()
            .map(|u| UserId(u.to_string()))
            .collect();
        user_ids.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(user_ids)
    }
}
