//! End-to-end encryption management: recovery (secret storage + key backup), cross-signing
//! bootstrap and interactive SAS (emoji) verification.
//!
//! Owned by stream core-crypto.

use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{RecoveryState, SasEmoji, UserId, VerificationState};
use futures::StreamExt;
use matrix_sdk::encryption::recovery::RecoveryState as SdkRecoveryState;
use matrix_sdk::encryption::verification::{
    SasState, SasVerification, Verification, VerificationRequest, VerificationRequestState,
};
use matrix_sdk::ruma::api::client::uiaa::{
    AuthData, MatrixUserIdentifier, Password, UserIdentifier,
};
use std::sync::Arc;

/// The single verification flow in progress, if any.
#[derive(Default)]
pub struct ActiveVerification {
    request: Option<VerificationRequest>,
    sas: Option<SasVerification>,
}

/// Verification progress pushed to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationUpdate {
    pub user_id: UserId,
    /// Id of the other device once known, else empty.
    pub device_id: String,
    pub state: VerificationState,
}

/// Callback receiving verification updates.
pub type VerificationCallback = Arc<dyn Fn(VerificationUpdate) + Send + Sync>;

fn sdk<E: std::fmt::Display>(e: E) -> AlloyError {
    AlloyError::MatrixSdk(e.to_string())
}

fn map_recovery_state(s: SdkRecoveryState) -> RecoveryState {
    match s {
        SdkRecoveryState::Unknown => RecoveryState::Unknown,
        SdkRecoveryState::Enabled => RecoveryState::Enabled,
        SdkRecoveryState::Disabled => RecoveryState::Disabled,
        // Secret storage exists but this device lacks the secrets: needs the recovery key.
        SdkRecoveryState::Incomplete => RecoveryState::Lost,
    }
}

impl AlloyClient {
    // ---------------------------------------------------------------- recovery

    /// Current recovery (secret storage) state.
    pub fn recovery_state(&self) -> RecoveryState {
        map_recovery_state(self.client.encryption().recovery().state())
    }

    /// Call `on_state` with the current and every later recovery state until aborted.
    pub async fn run_recovery_state(&self, mut on_state: impl FnMut(RecoveryState) + Send) {
        let mut stream = self.client.encryption().recovery().state_stream();
        while let Some(s) = stream.next().await {
            on_state(map_recovery_state(s));
        }
    }

    /// Bootstrap cross-signing if missing, answering the password UIA challenge when needed.
    pub async fn ensure_cross_signing(&self, password: Option<&str>) -> AlloyResult<()> {
        let enc = self.client.encryption();
        match enc.bootstrap_cross_signing_if_needed(None).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let Some(info) = e.as_uiaa_response() else {
                    return Err(sdk(e));
                };
                let password = password.ok_or_else(|| {
                    AlloyError::Auth("password required to set up cross-signing".into())
                })?;
                let user = self.client.user_id().ok_or(AlloyError::NotLoggedIn)?;
                let mut pw = Password::new(
                    UserIdentifier::Matrix(MatrixUserIdentifier::new(user.to_string())),
                    password.to_owned(),
                );
                pw.session = info.session.clone();
                enc.bootstrap_cross_signing_if_needed(Some(AuthData::Password(pw)))
                    .await
                    .map_err(sdk)
            }
        }
    }

    /// Set up secret storage + backup (and cross-signing if needed). Returns the recovery key,
    /// which must be shown to the user exactly once.
    pub async fn enable_recovery(&self, password: Option<&str>) -> AlloyResult<String> {
        self.ensure_cross_signing(password).await?;
        self.client
            .encryption()
            .recovery()
            .enable()
            .await
            .map_err(sdk)
    }

    /// Restore secrets (and thereby verify this device) using a recovery key or passphrase.
    pub async fn recover_with_key(&self, recovery_key: &str) -> AlloyResult<()> {
        self.client
            .encryption()
            .recovery()
            .recover(recovery_key.trim())
            .await
            .map_err(|e| AlloyError::Auth(e.to_string()))
    }

    /// Replace the recovery key (keeps cross-signing). Returns the new key.
    pub async fn reset_recovery(&self, password: Option<&str>) -> AlloyResult<String> {
        self.ensure_cross_signing(password).await?;
        self.client
            .encryption()
            .recovery()
            .reset_key()
            .await
            .map_err(sdk)
    }

    // ------------------------------------------------------------ verification

    /// Register a handler that reports verification requests from other devices of this account
    /// (and other users). The request becomes the active verification; call
    /// [`AlloyClient::accept_verification`] + [`AlloyClient::run_verification`] to proceed.
    pub fn watch_verification_requests(
        &self,
        on_request: Arc<dyn Fn(VerificationUpdate) + Send + Sync>,
    ) {
        use matrix_sdk::ruma::events::key::verification::request::ToDeviceKeyVerificationRequestEvent;
        let pending = self.pending_incoming.clone();
        self.client.add_event_handler(
            move |ev: ToDeviceKeyVerificationRequestEvent, c: matrix_sdk::Client| {
                let on_request = on_request.clone();
                let pending = pending.clone();
                async move {
                    if let Some(req) = c
                        .encryption()
                        .get_verification_request(&ev.sender, &ev.content.transaction_id)
                        .await
                    {
                        on_request(VerificationUpdate {
                            user_id: UserId(ev.sender.to_string()),
                            device_id: ev.content.from_device.to_string(),
                            state: VerificationState::Ready,
                        });
                        pending.lock().await.replace(req);
                    }
                }
            },
        );
    }

    /// Take the request captured by [`AlloyClient::watch_verification_requests`] as the active one.
    pub async fn adopt_incoming_verification(&self) -> AlloyResult<()> {
        let req = self
            .pending_incoming
            .lock()
            .await
            .take()
            .ok_or_else(|| AlloyError::MatrixSdk("no incoming verification request".into()))?;
        let mut active = self.verification.lock().await;
        active.request = Some(req);
        active.sas = None;
        Ok(())
    }

    /// Ask our other devices to verify this one. Becomes the active verification.
    pub async fn request_self_verification(&self) -> AlloyResult<()> {
        let me = self.client.user_id().ok_or(AlloyError::NotLoggedIn)?;
        let identity = self
            .client
            .encryption()
            .get_user_identity(me)
            .await
            .map_err(sdk)?
            .ok_or_else(|| {
                AlloyError::MatrixSdk(
                    "cross-signing is not set up for this account; enable recovery first".into(),
                )
            })?;
        let request = identity.request_verification().await.map_err(sdk)?;
        let mut active = self.verification.lock().await;
        active.request = Some(request);
        active.sas = None;
        Ok(())
    }

    async fn active_request(&self) -> AlloyResult<VerificationRequest> {
        self.verification
            .lock()
            .await
            .request
            .clone()
            .ok_or_else(|| AlloyError::MatrixSdk("no verification in progress".into()))
    }

    /// Accept an incoming request.
    pub async fn accept_verification(&self) -> AlloyResult<()> {
        self.active_request().await?.accept().await.map_err(sdk)
    }

    /// Start the emoji flow (done automatically by [`AlloyClient::run_verification`] for
    /// requests we started).
    pub async fn start_sas(&self) -> AlloyResult<()> {
        self.active_request()
            .await?
            .start_sas()
            .await
            .map_err(sdk)?;
        Ok(())
    }

    /// Confirm that the emojis match.
    pub async fn confirm_sas(&self) -> AlloyResult<()> {
        let sas = self
            .verification
            .lock()
            .await
            .sas
            .clone()
            .ok_or_else(|| AlloyError::MatrixSdk("no emoji verification in progress".into()))?;
        sas.confirm().await.map_err(sdk)
    }

    /// Cancel, or report a mismatch when the emoji flow is running.
    pub async fn cancel_verification(&self) -> AlloyResult<()> {
        let (req, sas) = {
            let a = self.verification.lock().await;
            (a.request.clone(), a.sas.clone())
        };
        if let Some(sas) = sas {
            return sas.mismatch().await.map_err(sdk);
        }
        match req {
            Some(r) => r.cancel().await.map_err(sdk),
            None => Ok(()),
        }
    }

    /// Drive the active verification to completion, reporting each step. Runs until the flow
    /// is done or cancelled (spawn it and abort to stop early).
    pub async fn run_verification(&self, on_update: VerificationCallback) -> AlloyResult<()> {
        let request = self.active_request().await?;
        let user_id = UserId(request.other_user_id().to_string());
        let mut device_id = String::new();
        let emit = |device_id: &str, state: VerificationState| {
            on_update(VerificationUpdate {
                user_id: user_id.clone(),
                device_id: device_id.to_owned(),
                state,
            })
        };
        let mut changes =
            futures::stream::once(futures::future::ready(request.state())).chain(request.changes());
        while let Some(state) = changes.next().await {
            match state {
                VerificationRequestState::Created { .. } => {
                    emit(&device_id, VerificationState::WaitingForPartner)
                }
                VerificationRequestState::Requested {
                    other_device_data, ..
                } => {
                    device_id = other_device_data.device_id().to_string();
                    emit(&device_id, VerificationState::Ready)
                }
                VerificationRequestState::Ready {
                    other_device_data, ..
                } => {
                    device_id = other_device_data.device_id().to_string();
                    if request.we_started() {
                        request.start_sas().await.map_err(sdk)?;
                        emit(&device_id, VerificationState::WaitingForPartner);
                    } else {
                        emit(&device_id, VerificationState::Ready);
                    }
                }
                VerificationRequestState::Transitioned { verification } => {
                    if let Verification::SasV1(sas) = verification {
                        self.verification.lock().await.sas = Some(sas.clone());
                        self.drive_sas(sas, &user_id, &device_id, &on_update)
                            .await?;
                    }
                    return Ok(());
                }
                VerificationRequestState::Done => {
                    emit(&device_id, VerificationState::Verified);
                    return Ok(());
                }
                VerificationRequestState::Cancelled(info) => {
                    emit(
                        &device_id,
                        VerificationState::Cancelled {
                            reason: info.reason().to_owned(),
                        },
                    );
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    async fn drive_sas(
        &self,
        sas: SasVerification,
        user_id: &UserId,
        device_id: &str,
        on_update: &VerificationCallback,
    ) -> AlloyResult<()> {
        let emit = |state: VerificationState| {
            on_update(VerificationUpdate {
                user_id: user_id.clone(),
                device_id: device_id.to_owned(),
                state,
            })
        };
        // `changes()` only yields future transitions; the flow may already be past `Started`.
        let mut changes =
            futures::stream::once(futures::future::ready(sas.state())).chain(sas.changes());
        let mut accepted = false;
        while let Some(state) = changes.next().await {
            match state {
                SasState::Started { .. } => {
                    if !sas.we_started() && !accepted {
                        accepted = true;
                        sas.accept().await.map_err(sdk)?;
                    }
                    emit(VerificationState::WaitingForPartner);
                }
                SasState::Created { .. } | SasState::Accepted { .. } => {
                    emit(VerificationState::WaitingForPartner)
                }
                SasState::KeysExchanged { emojis, decimals } => {
                    let emojis = emojis
                        .map(|e| {
                            e.emojis
                                .iter()
                                .zip(e.indices.iter())
                                .map(|(emoji, idx)| SasEmoji {
                                    emoji: emoji.symbol.to_owned(),
                                    decimal: u32::from(*idx),
                                    description: emoji.description.to_owned(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    emit(VerificationState::CompareEmojis {
                        emojis,
                        decimal_1: u32::from(decimals.0),
                        decimal_2: u32::from(decimals.1),
                        decimal_3: u32::from(decimals.2),
                    });
                }
                SasState::Confirmed => emit(VerificationState::WaitingForPartner),
                SasState::Done { .. } => {
                    emit(VerificationState::Verified);
                    return Ok(());
                }
                SasState::Cancelled(info) => {
                    emit(VerificationState::Cancelled {
                        reason: info.reason().to_owned(),
                    });
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}
