//! Own profile, the account's device list, and this session's crypto identity.

use std::path::Path;

use alloy_protocol::{DeviceDto, OwnProfileDto, SessionInfoDto, UserId};

use crate::{AlloyClient, AlloyError, AlloyResult};

fn sdk_err(e: impl std::fmt::Display) -> AlloyError {
    AlloyError::MatrixSdk(e.to_string())
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
        let device_id = self
            .client
            .device_id()
            .ok_or(AlloyError::NotLoggedIn)?
            .to_string();
        let encryption = self.client.encryption();
        let cross_signing_ready = encryption
            .cross_signing_status()
            .await
            .is_some_and(|status| status.is_complete());
        Ok(SessionInfoDto {
            device_id,
            ed25519_key: encryption.ed25519_key().await,
            curve25519_key: encryption.curve25519_key().await.map(|key| key.to_base64()),
            cross_signing_ready,
            backup_enabled: encryption.backups().are_enabled().await,
        })
    }
}
