//! The per-account outbox worker: durable, ordered, retried with backoff.
//!
//! Every user action that changes network state is first written to the store's `outbox` table by
//! the command handler; this worker claims due rows, performs them through the account's connector
//! and records the outcome. A retryable failure keeps the row pending with an exponential delay; a
//! permanent one marks the local message failed so the UI can offer RetrySend.

use std::sync::Arc;
use std::time::Duration;

use alloy_connector::{ConnectorError, LocalAttachment, OutgoingMessage};
use alloy_store::{OutboxItem, OutboxOp, SendState, StoreError};
use serde::Deserialize;
use tokio::time::Instant;

use crate::accounts::{AccountRuntime, retry_delay};
use crate::context::EngineContext;
use crate::error::map_store;
use crate::support::{OutboxAttachment, mark_message_failed, now_ms};

/// How many due rows one batch claims.
const CLAIM_BATCH: u32 = 32;
/// How long a row of an account that is not connected waits before another attempt.
const MISSING_ACCOUNT_RETRY_MS: i64 = 30_000;
/// Upper bound for a network-requested retry delay (a `FLOOD_WAIT` measured in hours is honoured
/// as one hour, not as an unclaimable row).
const MAX_RETRY_DELAY_MS: i64 = 3_600_000;
/// How long an idle worker sleeps before looking for rows whose retry time arrived while it waited.
///
/// The store has no "next due row" query, so the worker cannot know a future retry exists until it
/// looks; the tick bounds that blindness to one interval instead of polling per row.
const OUTBOX_IDLE_RECHECK: Duration = Duration::from_secs(30);

/// What a failed outbox row does next.
enum PerformError {
    /// The same call may succeed later; the row returns to `pending` at the given time.
    Retry {
        message: String,
        next_attempt_at: i64,
    },
    /// Retrying cannot help; the row is parked and the local message is marked failed.
    Fatal { message: String, unauthorized: bool },
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SendPayload {
    body: Option<String>,
    reply_to: Option<String>,
    attachment: Option<OutboxAttachment>,
}

#[derive(Debug, Deserialize)]
struct EditPayload {
    event_id: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct DeletePayload {
    event_id: String,
    for_everyone: bool,
}

#[derive(Debug, Deserialize)]
struct ReactionPayload {
    event_id: String,
    key: String,
}

#[derive(Debug, Deserialize)]
struct ReadPayload {
    event_id: String,
}

#[derive(Debug, Deserialize)]
struct TypingPayload {
    typing: bool,
}

/// Drains the account's outbox until the task is aborted.
///
/// Rows left `in_flight` by a crash are recovered once per engine start (or sync refresh), before
/// any worker runs: `reset_in_flight_outbox` is store-wide, so calling it here would also release
/// rows a sibling account's worker is mid-way through.
pub async fn run(ctx: EngineContext, runtime: Arc<AccountRuntime>) {
    let mut next_wake: Option<Instant> = None;
    loop {
        let claimed = ctx.store.claim_due_outbox(now_ms(), CLAIM_BATCH).await;
        let items = match claimed {
            Ok(items) => items,
            Err(err) => {
                tracing::warn!(account = %runtime.account_id, error = %err, "cannot claim outbox rows");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        for item in items {
            let chat_id = item.chat_id.clone();
            // The store claims due rows across every account, so a worker can pick up a row that
            // belongs to a sibling account. Route it through that account's own connector instead
            // of sending it with the wrong credentials.
            let Some(owner) = ctx.accounts.get(&item.account_id).await else {
                let retry_at = now_ms().saturating_add(MISSING_ACCOUNT_RETRY_MS);
                if let Err(err) = ctx
                    .store
                    .fail_outbox(&item.txn_id, "the account is not connected", retry_at)
                    .await
                {
                    tracing::warn!(txn = %item.txn_id, error = %err, "cannot requeue an outbox row");
                }
                let wake = instant_at(retry_at);
                next_wake = Some(next_wake.map_or(wake, |scheduled| scheduled.min(wake)));
                continue;
            };
            match perform(&ctx, &owner, &item).await {
                Ok(()) => {
                    if let Err(err) = ctx.store.complete_outbox(&item.txn_id).await {
                        tracing::warn!(txn = %item.txn_id, error = %err, "cannot complete an outbox row");
                    }
                    ctx.bus.timeline(&chat_id);
                    ctx.bus.chats();
                }
                Err(PerformError::Retry {
                    message,
                    next_attempt_at,
                }) => {
                    if let Err(err) = ctx
                        .store
                        .fail_outbox(&item.txn_id, &message, next_attempt_at)
                        .await
                    {
                        tracing::warn!(txn = %item.txn_id, error = %err, "cannot schedule an outbox retry");
                    }
                    let wake = instant_at(next_attempt_at);
                    next_wake = Some(next_wake.map_or(wake, |scheduled| scheduled.min(wake)));
                    tracing::warn!(txn = %item.txn_id, error = %message, "outbox row will retry");
                }
                Err(PerformError::Fatal {
                    message,
                    unauthorized,
                }) => {
                    if let Err(err) = ctx
                        .store
                        .fail_outbox(&item.txn_id, &message, i64::MAX)
                        .await
                    {
                        tracing::warn!(txn = %item.txn_id, error = %err, "cannot park an outbox row");
                    }
                    record_failure(&ctx, &item.txn_id, &message).await;
                    ctx.bus.timeline(&chat_id);
                    tracing::warn!(txn = %item.txn_id, error = %message, "outbox row failed permanently");
                    if unauthorized {
                        ctx.sink
                            .emit(alloy_protocol::EngineEvent::ConnectionStatus {
                                account_id: alloy_protocol::AccountId(owner.account_id.clone()),
                                status: alloy_protocol::ConnectionStatus::Terminated,
                            });
                        crate::accounts::mark_needs_relogin(&ctx, &owner.account_id).await;
                    }
                }
            }
        }

        match next_wake.take() {
            Some(wake) => {
                tokio::select! {
                    () = runtime.wait_outbox() => {}
                    () = tokio::time::sleep_until(wake) => {}
                }
            }
            None => {
                tokio::select! {
                    () = runtime.wait_outbox() => {}
                    () = tokio::time::sleep(OUTBOX_IDLE_RECHECK) => {}
                }
            }
        }
    }
}

async fn perform(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    item: &OutboxItem,
) -> Result<(), PerformError> {
    let chat = ctx
        .store
        .get_chat(&item.chat_id)
        .await
        .map_err(store_failure)?
        .ok_or_else(|| fatal("the chat no longer exists"))?;

    match item.op {
        OutboxOp::Send => send(ctx, runtime, item, &chat.remote_chat_id).await,
        OutboxOp::Edit => edit(ctx, runtime, item, &chat.remote_chat_id).await,
        OutboxOp::Delete => delete(ctx, runtime, item, &chat.remote_chat_id).await,
        OutboxOp::React | OutboxOp::Unreact => {
            react(ctx, runtime, item, &chat.remote_chat_id).await
        }
        OutboxOp::Read => read(ctx, runtime, item, &chat.remote_chat_id).await,
        OutboxOp::Typing => typing(runtime, item, &chat.remote_chat_id).await,
    }
}

async fn send(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    item: &OutboxItem,
    remote_chat_id: &str,
) -> Result<(), PerformError> {
    let payload: SendPayload = serde_json::from_value(item.payload.clone())
        .map_err(|err| fatal(format!("malformed send payload: {err}")))?;
    let record = ctx
        .store
        .find_message_by_txn(&item.txn_id)
        .await
        .map_err(store_failure)?
        .ok_or_else(|| fatal("the queued message is gone"))?;

    if record.send_state == Some(SendState::Queued) {
        ctx.store
            .set_send_state(&record.message_id, SendState::Sending, None, None)
            .await
            .map_err(store_failure)?;
    }

    // A copy the engine made only for this upload is deleted once the outcome is final; a
    // retryable failure keeps it for the next attempt.
    let temporary_attachment = payload
        .attachment
        .as_ref()
        .filter(|attachment| attachment.temporary)
        .map(|attachment| attachment.path.clone());
    let outgoing = OutgoingMessage {
        txn_id: item.txn_id.clone(),
        remote_chat_id: remote_chat_id.to_string(),
        body: payload.body,
        formatted: None,
        reply_to: payload.reply_to,
        attachment: payload.attachment.map(|attachment| LocalAttachment {
            path: attachment.path,
            mime: attachment.mime,
            file_name: attachment.file_name,
        }),
    };

    let sent = runtime.connector.send(outgoing).await;
    if let Some(path) = &temporary_attachment {
        let will_retry = matches!(&sent, Err(err) if err.is_retryable());
        if !will_retry {
            remove_stored_file(path);
        }
    }
    match sent {
        Ok(ack) => {
            ctx.store
                .set_send_state(
                    &record.message_id,
                    SendState::Sent,
                    Some(ack.remote_msg_id),
                    Some(ack.sent_at),
                )
                .await
                .map_err(store_failure)?;
            ctx.store
                .update_chat_meta(
                    &item.chat_id,
                    None,
                    None,
                    Some(record.message_id),
                    Some(ack.sent_at),
                )
                .await
                .map_err(store_failure)?;
            Ok(())
        }
        Err(err) => Err(classify(err, item.attempts)),
    }
}

async fn edit(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    item: &OutboxItem,
    remote_chat_id: &str,
) -> Result<(), PerformError> {
    let payload: EditPayload = serde_json::from_value(item.payload.clone())
        .map_err(|err| fatal(format!("malformed edit payload: {err}")))?;
    let record = ctx
        .store
        .get_message(&payload.event_id)
        .await
        .map_err(store_failure)?
        .ok_or_else(|| fatal("the edited message is gone"))?;
    let remote_msg_id = record
        .remote_msg_id
        .clone()
        .ok_or_else(|| fatal("the message has no network id"))?;

    match runtime
        .connector
        .edit(remote_chat_id, &remote_msg_id, payload.body.clone(), None)
        .await
    {
        Ok(()) => {
            ctx.store
                .edit_message_body(&record.message_id, Some(payload.body), None, now_ms())
                .await
                .map_err(store_failure)?;
            Ok(())
        }
        Err(err) => Err(classify(err, item.attempts)),
    }
}

async fn delete(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    item: &OutboxItem,
    remote_chat_id: &str,
) -> Result<(), PerformError> {
    let payload: DeletePayload = serde_json::from_value(item.payload.clone())
        .map_err(|err| fatal(format!("malformed delete payload: {err}")))?;
    let record = ctx
        .store
        .get_message(&payload.event_id)
        .await
        .map_err(store_failure)?
        .ok_or_else(|| fatal("the deleted message is gone"))?;
    let remote_msg_id = record
        .remote_msg_id
        .clone()
        .ok_or_else(|| fatal("the message has no network id"))?;

    match runtime
        .connector
        .delete(remote_chat_id, &remote_msg_id, payload.for_everyone)
        .await
    {
        Ok(()) => {
            ctx.store
                .mark_deleted(&record.message_id, now_ms())
                .await
                .map_err(store_failure)?;
            Ok(())
        }
        Err(err) => Err(classify(err, item.attempts)),
    }
}

async fn react(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    item: &OutboxItem,
    remote_chat_id: &str,
) -> Result<(), PerformError> {
    let payload: ReactionPayload = serde_json::from_value(item.payload.clone())
        .map_err(|err| fatal(format!("malformed reaction payload: {err}")))?;
    let record = ctx
        .store
        .get_message(&payload.event_id)
        .await
        .map_err(store_failure)?
        .ok_or_else(|| fatal("the reacted message is gone"))?;
    let remote_msg_id = record
        .remote_msg_id
        .clone()
        .ok_or_else(|| fatal("the message has no network id"))?;
    let own = runtime
        .remote_user_id
        .clone()
        .ok_or_else(|| fatal("the account has no signed-in user"))?;

    let result = match item.op {
        OutboxOp::React => {
            runtime
                .connector
                .react(remote_chat_id, &remote_msg_id, &payload.key)
                .await
        }
        _ => {
            runtime
                .connector
                .unreact(remote_chat_id, &remote_msg_id, &payload.key)
                .await
        }
    };

    match result {
        Ok(()) => {
            if item.op == OutboxOp::React {
                ctx.store
                    .set_reaction(&record.message_id, &own, &payload.key, now_ms())
                    .await
                    .map_err(store_failure)?;
            } else {
                ctx.store
                    .remove_reaction(&record.message_id, &own, &payload.key)
                    .await
                    .map_err(store_failure)?;
            }
            Ok(())
        }
        Err(err) => Err(classify(err, item.attempts)),
    }
}

async fn read(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    item: &OutboxItem,
    remote_chat_id: &str,
) -> Result<(), PerformError> {
    let payload: ReadPayload = serde_json::from_value(item.payload.clone())
        .map_err(|err| fatal(format!("malformed read payload: {err}")))?;
    let record = match ctx
        .store
        .get_message(&payload.event_id)
        .await
        .map_err(store_failure)?
    {
        Some(record) if record.chat_id == item.chat_id => Some(record),
        _ => None,
    };

    // A deleted message can no longer serve as the receipt anchor; the newest remaining message
    // of the chat is the next best position, and an empty chat has nothing to acknowledge.
    let remote_msg_id = match record
        .as_ref()
        .and_then(|record| record.remote_msg_id.clone())
    {
        Some(remote) => remote,
        None => match ctx
            .store
            .page_messages(&item.chat_id, None, 1)
            .await
            .map_err(store_failure)?
            .into_iter()
            .next()
            .and_then(|record| record.remote_msg_id)
        {
            Some(remote) => remote,
            None => return Ok(()),
        },
    };

    runtime
        .connector
        .mark_read(remote_chat_id, &remote_msg_id)
        .await
        .map_err(|err| classify(err, item.attempts))?;

    if let (Some(record), Some(own)) = (record, runtime.remote_user_id.clone()) {
        ctx.store
            .set_read_marker(&item.chat_id, &own, &record.message_id, now_ms())
            .await
            .map_err(store_failure)?;
    }
    Ok(())
}

/// Typing indicators are ephemeral: a failure is dropped instead of being retried or surfaced.
async fn typing(
    runtime: &AccountRuntime,
    item: &OutboxItem,
    remote_chat_id: &str,
) -> Result<(), PerformError> {
    let payload: TypingPayload = serde_json::from_value(item.payload.clone())
        .map_err(|err| fatal(format!("malformed typing payload: {err}")))?;
    if let Err(err) = runtime
        .connector
        .set_typing(remote_chat_id, payload.typing)
        .await
    {
        tracing::debug!(txn = %item.txn_id, error = %err, "typing indicator was not delivered");
    }
    Ok(())
}

fn classify(err: ConnectorError, attempts: i64) -> PerformError {
    match err {
        ConnectorError::RateLimited { retry_after_ms } => PerformError::Retry {
            message: format!("rate limited: retry after {retry_after_ms} ms"),
            next_attempt_at: now_ms().saturating_add(
                i64::try_from(retry_after_ms)
                    .unwrap_or(MAX_RETRY_DELAY_MS)
                    .min(MAX_RETRY_DELAY_MS),
            ),
        },
        other if other.is_retryable() => PerformError::Retry {
            message: other.to_string(),
            next_attempt_at: now_ms().saturating_add(
                i64::try_from(retry_delay(attempts).as_millis()).unwrap_or(i64::MAX),
            ),
        },
        ConnectorError::Unauthorized => PerformError::Fatal {
            message: "the stored session is no longer valid".to_string(),
            unauthorized: true,
        },
        other => PerformError::Fatal {
            message: other.to_string(),
            unauthorized: false,
        },
    }
}

/// Marks the local message behind a failed outbox row, if the row has one.
async fn record_failure(ctx: &EngineContext, txn_id: &str, message: &str) {
    let Ok(Some(record)) = ctx.store.find_message_by_txn(txn_id).await else {
        return;
    };
    if let Err(err) = mark_message_failed(ctx, &record.message_id, message).await {
        tracing::warn!(message = %record.message_id, error = %err.message, "cannot record the send failure");
    }
}

fn fatal(message: impl Into<String>) -> PerformError {
    PerformError::Fatal {
        message: message.into(),
        unauthorized: false,
    }
}

fn store_failure(err: StoreError) -> PerformError {
    fatal(map_store(err).message)
}

fn remove_stored_file(path: &str) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => tracing::warn!(path = %path, error = %err, "cannot remove an uploaded file"),
    }
}

fn instant_at(timestamp_ms: i64) -> Instant {
    let delay = (timestamp_ms - now_ms()).clamp(0, 3_600_000);
    Instant::now() + Duration::from_millis(u64::try_from(delay).unwrap_or(0))
}
