use super::rooms::map_err;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_core::{AlloyClient, VerificationUpdate};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse, SubscriptionId, UserId};
use std::sync::Arc;

/// Handler for encryption and verification operations (`CommandGroup::Crypto`).
///
/// Owned by stream: `core-crypto`
#[derive(Default)]
pub struct CryptoHandler;

impl CryptoHandler {
    pub fn new() -> Self {
        Self
    }
}

fn emit_update(ctx: &EngineContext, u: VerificationUpdate) {
    ctx.sink.emit(EngineEvent::VerificationState {
        user_id: u.user_id,
        device_id: u.device_id,
        state: u.state,
    });
}

/// Drive the client's active verification in the background, emitting `VerificationState`.
async fn spawn_verification_driver(ctx: &EngineContext, client: Arc<AlloyClient>) {
    let sink = ctx.sink.clone();
    let id = SubscriptionId(format!("verification-{}", uuid::Uuid::new_v4()));
    let task = tokio::spawn(async move {
        let cb: alloy_core::VerificationCallback = Arc::new(move |u: VerificationUpdate| {
            sink.emit(EngineEvent::VerificationState {
                user_id: u.user_id,
                device_id: u.device_id,
                state: u.state,
            })
        });
        let _ = client.run_verification(cb).await;
    });
    ctx.subscriptions.register(id, task.abort_handle()).await;
}

/// Start background crypto work for a freshly activated client: sync, recovery-state events and
/// incoming verification requests. Called by the auth handler after login/restore.
pub(crate) async fn start_background(ctx: &EngineContext, client: Arc<AlloyClient>) {
    // Registered before sync so synced preferences in the initial sync are not missed.
    let prefs_sink = ctx.sink.clone();
    client.watch_account_prefs(Arc::new(move |json: String| {
        prefs_sink.emit(EngineEvent::AccountPrefsChanged { json })
    }));

    // Sync must run for key and verification traffic to flow.
    let _ = client.sync_service().await;

    let sink = ctx.sink.clone();
    let c = client.clone();
    let task = tokio::spawn(async move {
        c.run_recovery_state(|state| sink.emit(EngineEvent::RecoveryState { state }))
            .await;
    });
    ctx.subscriptions
        .register(
            SubscriptionId(format!("recovery-{}", uuid::Uuid::new_v4())),
            task.abort_handle(),
        )
        .await;

    let weak = Arc::downgrade(&client);
    let ctx2 = ctx.clone();
    client.watch_verification_requests(Arc::new(move |u: VerificationUpdate| {
        let (weak, ctx) = (weak.clone(), ctx2.clone());
        tokio::spawn(async move {
            let Some(client) = weak.upgrade() else { return };
            if client.adopt_incoming_verification().await.is_ok() {
                emit_update(&ctx, u);
                spawn_verification_driver(&ctx, client).await;
            }
        });
    }));
}

#[async_trait::async_trait]
impl GroupHandler for CryptoHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Crypto
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::GetRecoveryStatus => {
                let client = ctx.active_client().await?;
                Ok(EngineResponse::RecoveryStatusRetrieved {
                    state: client.recovery_state(),
                })
            }
            EngineCommand::EnableRecovery { password } => {
                let client = ctx.active_client().await?;
                let recovery_key = client
                    .enable_recovery(password.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RecoveryEnabled { recovery_key })
            }
            EngineCommand::ResetRecovery { password } => {
                let client = ctx.active_client().await?;
                let recovery_key = client
                    .reset_recovery(password.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RecoveryReset { recovery_key })
            }
            EngineCommand::RecoverWithKey { recovery_key } => {
                let client = ctx.active_client().await?;
                client
                    .recover_with_key(&recovery_key)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RecoveredFromKey { recovered: true })
            }
            EngineCommand::RequestSelfVerification => {
                let client = ctx.active_client().await?;
                client.request_self_verification().await.map_err(map_err)?;
                let user_id = client.user_id().unwrap_or_default().to_owned();
                spawn_verification_driver(ctx, client).await;
                Ok(EngineResponse::VerificationRequested {
                    user_id: UserId(user_id),
                    device_id: String::new(),
                })
            }
            EngineCommand::AcceptVerification { user_id, device_id } => {
                let client = ctx.active_client().await?;
                client.accept_verification().await.map_err(map_err)?;
                Ok(EngineResponse::VerificationAccepted { user_id, device_id })
            }
            EngineCommand::StartSas { user_id, device_id } => {
                let client = ctx.active_client().await?;
                client.start_sas().await.map_err(map_err)?;
                Ok(EngineResponse::SasStarted {
                    user_id,
                    device_id,
                    emojis: vec![],
                })
            }
            EngineCommand::ConfirmSas { user_id, device_id } => {
                let client = ctx.active_client().await?;
                client.confirm_sas().await.map_err(map_err)?;
                Ok(EngineResponse::SasConfirmed { user_id, device_id })
            }
            EngineCommand::CancelVerification { user_id, device_id } => {
                let client = ctx.active_client().await?;
                client.cancel_verification().await.map_err(map_err)?;
                Ok(EngineResponse::VerificationCancelled { user_id, device_id })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Crypto group",
                name
            ))),
        }
    }
}
