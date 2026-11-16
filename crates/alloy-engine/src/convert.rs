//! Mapping between the three vocabularies the engine straddles: `alloy-store` rows,
//! `alloy-connector` values and `alloy-protocol` DTOs.
//!
//! The three crates deliberately do not depend on each other, so every crossing goes through this
//! one module instead of ad-hoc `From` impls scattered across handlers.

use alloy_connector::capabilities::Capabilities as ConnectorCapabilities;
use alloy_connector::events::ConnectionState;
use alloy_connector::login::{
    ChallengeKind as ConnectorChallengeKind, CodeChannel as ConnectorCodeChannel, CookieField,
    LoginFailure,
};
use alloy_connector::types::{ChatKind as ConnectorChatKind, MessageKind as ConnectorMessageKind};
use alloy_protocol::{
    AccountState as ProtocolAccountState, CapabilitiesDto, ChallengeKind as ProtocolChallengeKind,
    ChatKind as ProtocolChatKind, CodeChannel as ProtocolCodeChannel, ConnectionStatus,
    CookieFieldDto, LoginAlternativeDto, LoginFailureDto, LoginInputDto, LoginStepDto,
    Network as ProtocolNetwork, NetworkInfoDto, NetworkSettingDto, NetworkSettingKind,
    NetworkSetupDto, NetworkSetupFieldDto, NetworkSetupKind,
    TelegramAppSource as ProtocolTelegramAppSource,
};
use alloy_store::{
    AccountState as StoreAccountState, ChatKind as StoreChatKind, MessageKind as StoreMessageKind,
    Network as StoreNetwork,
};

use crate::factory::{self, NetworkSettingSpec, NetworkSettings, TelegramAppSource};

// === Networks ===

pub fn network_store_to_protocol(network: StoreNetwork) -> ProtocolNetwork {
    match network {
        StoreNetwork::Telegram => ProtocolNetwork::Telegram,
        StoreNetwork::Messenger => ProtocolNetwork::Messenger,
        StoreNetwork::Fake => ProtocolNetwork::Fake,
    }
}

pub fn network_protocol_to_store(network: ProtocolNetwork) -> StoreNetwork {
    match network {
        ProtocolNetwork::Telegram => StoreNetwork::Telegram,
        ProtocolNetwork::Messenger => StoreNetwork::Messenger,
        ProtocolNetwork::Fake => StoreNetwork::Fake,
    }
}

pub fn network_label(network: ProtocolNetwork) -> &'static str {
    match network {
        ProtocolNetwork::Telegram => "telegram",
        ProtocolNetwork::Messenger => "messenger",
        ProtocolNetwork::Fake => "fake",
    }
}

/// Where a user registers the Telegram application pair.
pub const TELEGRAM_APP_URL: &str = "https://my.telegram.org/apps";

/// Telegram as `ListNetworks` reports it: always listed, ready once a pair is configured.
///
/// Without a pair the setup block names the two values the user has to enter and where to get
/// them, so `BeginLogin` failing with `NotSupported` has actionable context.
pub fn telegram_network_info(configured: bool) -> NetworkInfoDto {
    let setup = (!configured).then(|| {
        let settings = network_setting_dtos(ProtocolNetwork::Telegram, &NetworkSettings::default());
        NetworkSetupDto {
            kind: NetworkSetupKind::TelegramApp,
            app_url: TELEGRAM_APP_URL.to_string(),
            credentials: vec![
                NetworkSetupFieldDto {
                    key: "api_id".to_string(),
                    label: "API ID".to_string(),
                    secret: false,
                },
                NetworkSetupFieldDto {
                    key: "api_hash".to_string(),
                    label: "API hash".to_string(),
                    secret: true,
                },
            ],
            settings,
        }
    });
    NetworkInfoDto {
        network: ProtocolNetwork::Telegram,
        ready: configured,
        setup,
    }
}

/// Messenger as `ListNetworks` reports it: listed only when the sidecar exists, always ready.
///
/// It asks for no credential, so its setup block carries settings only. A setting never changes
/// `ready`: a sidecar that exists can log in, and the settings say how it presents itself.
pub fn messenger_network_info(settings: &NetworkSettings) -> NetworkInfoDto {
    let setup = NetworkSetupDto {
        kind: NetworkSetupKind::Messenger,
        // Messenger registers no application in a browser.
        app_url: String::new(),
        credentials: Vec::new(),
        settings: network_setting_dtos(ProtocolNetwork::Messenger, settings),
    };
    NetworkInfoDto {
        network: ProtocolNetwork::Messenger,
        ready: true,
        setup: Some(setup),
    }
}

/// Every declared setting of `network`, with the value in effect.
pub fn network_setting_dtos(
    network: ProtocolNetwork,
    settings: &NetworkSettings,
) -> Vec<NetworkSettingDto> {
    factory::network_setting_specs(network)
        .iter()
        .map(|spec| network_setting_dto(network, spec, settings))
        .collect()
}

/// One declared setting with the value in effect.
///
/// A secret setting never carries a value: `ListNetworks` is not a way to read a credential back.
pub fn network_setting_dto(
    network: ProtocolNetwork,
    spec: &NetworkSettingSpec,
    settings: &NetworkSettings,
) -> NetworkSettingDto {
    NetworkSettingDto {
        key: spec.key.to_string(),
        label: spec.label.to_string(),
        kind: spec.kind,
        options: spec
            .options
            .iter()
            .map(|option| alloy_protocol::NetworkSettingOptionDto {
                value: option.value.to_string(),
                label: option.label.to_string(),
            })
            .collect(),
        value: if spec.kind == NetworkSettingKind::Secret {
            None
        } else {
            factory::effective_network_setting(settings, network, spec)
        },
    }
}

/// The source of the Telegram application pair, as `GetTelegramApp` reports it.
pub fn telegram_app_source(source: TelegramAppSource) -> ProtocolTelegramAppSource {
    match source {
        TelegramAppSource::Runtime => ProtocolTelegramAppSource::Runtime,
        TelegramAppSource::Environment => ProtocolTelegramAppSource::Environment,
    }
}

// === Accounts ===

pub fn account_state_store_to_protocol(state: StoreAccountState) -> ProtocolAccountState {
    match state {
        StoreAccountState::Pending => ProtocolAccountState::Pending,
        StoreAccountState::Active => ProtocolAccountState::Active,
        StoreAccountState::NeedsRelogin => ProtocolAccountState::NeedsRelogin,
        StoreAccountState::Disabled => ProtocolAccountState::Disabled,
    }
}

// === Chats and messages ===

pub fn chat_kind_store_to_protocol(kind: StoreChatKind) -> ProtocolChatKind {
    match kind {
        StoreChatKind::Direct => ProtocolChatKind::Direct,
        StoreChatKind::Group => ProtocolChatKind::Group,
        StoreChatKind::Channel => ProtocolChatKind::Channel,
    }
}

pub fn chat_kind_connector_to_store(kind: ConnectorChatKind) -> StoreChatKind {
    match kind {
        ConnectorChatKind::Direct => StoreChatKind::Direct,
        ConnectorChatKind::Group => StoreChatKind::Group,
        ConnectorChatKind::Channel => StoreChatKind::Channel,
    }
}

pub fn message_kind_connector_to_store(kind: ConnectorMessageKind) -> StoreMessageKind {
    match kind {
        ConnectorMessageKind::Text => StoreMessageKind::Text,
        ConnectorMessageKind::Image => StoreMessageKind::Image,
        ConnectorMessageKind::Video => StoreMessageKind::Video,
        ConnectorMessageKind::Audio => StoreMessageKind::Audio,
        ConnectorMessageKind::Voice => StoreMessageKind::Voice,
        ConnectorMessageKind::File => StoreMessageKind::File,
        ConnectorMessageKind::Sticker => StoreMessageKind::Sticker,
        ConnectorMessageKind::Location => StoreMessageKind::Location,
        ConnectorMessageKind::System => StoreMessageKind::System,
    }
}

// === Capabilities ===

pub fn capabilities_dto(capabilities: &ConnectorCapabilities) -> CapabilitiesDto {
    CapabilitiesDto {
        edit: capabilities.edit,
        edit_window_secs: capabilities.edit_window_secs,
        delete_for_everyone: capabilities.delete_for_everyone,
        reactions: capabilities.reactions,
        max_reactions_per_user: capabilities.max_reactions_per_user,
        replies: capabilities.replies,
        threads: capabilities.threads,
        typing: capabilities.typing,
        read_receipts: capabilities.read_receipts,
        delivered_receipts: capabilities.delivered_receipts,
        voice: capabilities.voice,
        polls: capabilities.polls,
        location: capabilities.location,
        max_upload_bytes: capabilities.max_upload_bytes,
        formatted_text: capabilities.formatted_text,
        pin_chats: capabilities.pin_chats,
        pin_messages: capabilities.pin_messages,
        topics: capabilities.topics,
    }
}

/// Capabilities of an account whose connector is not running.
///
/// No action works while the connector is down, so telling the UI that nothing is supported is the
/// truthful answer; a stale copy of the last capabilities would offer buttons that fail.
pub fn no_capabilities() -> CapabilitiesDto {
    CapabilitiesDto {
        edit: false,
        edit_window_secs: None,
        delete_for_everyone: false,
        reactions: false,
        max_reactions_per_user: None,
        replies: false,
        threads: false,
        typing: false,
        read_receipts: false,
        delivered_receipts: false,
        voice: false,
        polls: false,
        location: false,
        max_upload_bytes: None,
        formatted_text: false,
        pin_chats: false,
        pin_messages: false,
        topics: false,
    }
}

// === Connection state ===

pub fn connection_status(state: ConnectionState) -> ConnectionStatus {
    match state {
        ConnectionState::Connecting => ConnectionStatus::Connecting,
        ConnectionState::Online => ConnectionStatus::Connected,
        ConnectionState::Offline {
            attempt,
            next_retry_millis,
            ..
        } => {
            let attempt = attempt.unwrap_or(1).max(1);
            ConnectionStatus::Reconnecting {
                attempt,
                // The connector knows its own backoff; only guess when it did not say.
                next_retry_millis: next_retry_millis
                    .unwrap_or_else(|| reconnect_delay_millis(attempt)),
            }
        }
        ConnectionState::NeedsRelogin => ConnectionStatus::Terminated,
    }
}

/// Fallback delay before the attempt after `attempt` (1-based), for a connector that does not
/// report its own.
///
/// The connector retries on its own and [`ConnectionState::Offline`] now carries the delay it will
/// really wait, so this only covers a connector that stays silent about it. Messenger's loop
/// (`messagix` `Client.Connect`) doubles before it sleeps and stops at `MaxConnectBackoff`, so the
/// wait after failed attempt `n` is `min(2^n s, 5 min)` — already two seconds after the first
/// failure.
pub fn reconnect_delay_millis(attempt: u32) -> u64 {
    const MAX_MILLIS: u64 = 300_000;
    let doublings = attempt.clamp(1, 9);
    (1_000u64 << doublings).min(MAX_MILLIS)
}

// === Login ===

pub fn code_channel(channel: ConnectorCodeChannel) -> ProtocolCodeChannel {
    match channel {
        ConnectorCodeChannel::App => ProtocolCodeChannel::App,
        ConnectorCodeChannel::Sms => ProtocolCodeChannel::Sms,
        ConnectorCodeChannel::Email => ProtocolCodeChannel::Email,
        ConnectorCodeChannel::Other => ProtocolCodeChannel::Other,
    }
}

pub fn cookie_field(field: CookieField) -> CookieFieldDto {
    CookieFieldDto {
        name: field.name,
        label: field.label,
        required: field.required,
    }
}

pub fn login_failure(reason: LoginFailure) -> LoginFailureDto {
    match reason {
        LoginFailure::InvalidInput { field, message } => {
            LoginFailureDto::InvalidInput { field, message }
        }
        LoginFailure::Expired => LoginFailureDto::Expired,
        LoginFailure::NotSupported => LoginFailureDto::NotSupported,
        LoginFailure::RateLimited { retry_after_ms } => {
            LoginFailureDto::RateLimited { retry_after_ms }
        }
        LoginFailure::Network { message } => LoginFailureDto::Network { message },
        LoginFailure::AccountLocked { message } => LoginFailureDto::AccountLocked { message },
    }
}

fn challenge_kind(kind: ConnectorChallengeKind) -> ProtocolChallengeKind {
    match kind {
        ConnectorChallengeKind::Checkpoint => ProtocolChallengeKind::Checkpoint,
        ConnectorChallengeKind::TwoFactor => ProtocolChallengeKind::TwoFactor,
        ConnectorChallengeKind::Consent => ProtocolChallengeKind::Consent,
        ConnectorChallengeKind::UnsupportedMfa => ProtocolChallengeKind::UnsupportedMfa,
    }
}

/// Converts a login step that still needs user input.
///
/// [`alloy_connector::LoginStep::Done`] is not a question for the UI, so it returns `None` and the
/// caller finishes the account instead of rendering a step.
pub fn pending_login_step(step: alloy_connector::LoginStep) -> Option<LoginStepDto> {
    match step {
        alloy_connector::LoginStep::PhoneNumber => Some(LoginStepDto::PhoneNumber),
        alloy_connector::LoginStep::Code { via, length } => Some(LoginStepDto::Code {
            via: code_channel(via),
            length,
        }),
        alloy_connector::LoginStep::Password { hint } => Some(LoginStepDto::Password { hint }),
        alloy_connector::LoginStep::Qr {
            payload,
            expires_at,
        } => Some(LoginStepDto::Qr {
            payload,
            expires_at,
        }),
        alloy_connector::LoginStep::Cookies { fields, help_url } => Some(LoginStepDto::Cookies {
            fields: fields.into_iter().map(cookie_field).collect(),
            help_url,
        }),
        alloy_connector::LoginStep::Credentials {
            username_label,
            alternatives,
        } => Some(LoginStepDto::Credentials {
            username_label,
            alternatives: alternatives.into_iter().map(login_alternative).collect(),
        }),
        alloy_connector::LoginStep::Challenge { kind, message } => Some(LoginStepDto::Challenge {
            kind: challenge_kind(kind),
            message,
        }),
        alloy_connector::LoginStep::Wait { message } => Some(LoginStepDto::Wait { message }),
        alloy_connector::LoginStep::Done { .. } | alloy_connector::LoginStep::Failed { .. } => None,
    }
}

fn login_alternative(value: alloy_connector::LoginAlternative) -> LoginAlternativeDto {
    match value {
        alloy_connector::LoginAlternative::Cookies => LoginAlternativeDto::Cookies,
    }
}

fn connector_login_alternative(value: LoginAlternativeDto) -> alloy_connector::LoginAlternative {
    match value {
        LoginAlternativeDto::Cookies => alloy_connector::LoginAlternative::Cookies,
    }
}

pub fn login_input(input: LoginInputDto) -> alloy_connector::LoginInput {
    match input {
        LoginInputDto::PhoneNumber { phone } => alloy_connector::LoginInput::PhoneNumber { phone },
        LoginInputDto::Code { code } => alloy_connector::LoginInput::Code { code },
        LoginInputDto::Password { password } => alloy_connector::LoginInput::Password { password },
        LoginInputDto::Cookies { values } => alloy_connector::LoginInput::Cookies { values },
        LoginInputDto::Credentials { username, password } => {
            alloy_connector::LoginInput::Credentials { username, password }
        }
        LoginInputDto::Switch { to } => alloy_connector::LoginInput::Switch {
            to: connector_login_alternative(to),
        },
        LoginInputDto::QrRefresh => alloy_connector::LoginInput::QrRefresh,
        LoginInputDto::Continue => alloy_connector::LoginInput::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_delay_matches_the_messenger_loop() {
        // messagix doubles before sleeping and stops at MaxConnectBackoff (5 min): the wait after
        // failed attempt n is min(2^n s, 5 min), so the first retry already waits two seconds.
        assert_eq!(reconnect_delay_millis(1), 2_000);
        assert_eq!(reconnect_delay_millis(2), 4_000);
        assert_eq!(reconnect_delay_millis(3), 8_000);
        assert_eq!(reconnect_delay_millis(8), 256_000);
        assert_eq!(reconnect_delay_millis(9), 300_000);
        assert_eq!(reconnect_delay_millis(40), 300_000);
        // An attempt count of zero cannot happen on the wire, but it must not shift the base.
        assert_eq!(reconnect_delay_millis(0), 2_000);
    }

    #[test]
    fn a_wait_step_keeps_the_networks_instruction_and_continue_answers_it() {
        let step = pending_login_step(alloy_connector::LoginStep::Wait {
            message: "We sent a notification to your phone".to_string(),
        });
        assert_eq!(
            step,
            Some(LoginStepDto::Wait {
                message: "We sent a notification to your phone".to_string(),
            })
        );
        assert_eq!(
            login_input(LoginInputDto::Continue),
            alloy_connector::LoginInput::Continue
        );
    }

    #[test]
    fn offline_without_a_count_reports_the_first_attempt() {
        assert_eq!(
            connection_status(ConnectionState::Offline {
                reason: "network unreachable".to_string(),
                attempt: Some(3),
                next_retry_millis: None,
            }),
            ConnectionStatus::Reconnecting {
                attempt: 3,
                next_retry_millis: 8_000,
            }
        );
        assert_eq!(
            connection_status(ConnectionState::Offline {
                reason: "network unreachable".to_string(),
                attempt: None,
                next_retry_millis: None,
            }),
            ConnectionStatus::Reconnecting {
                attempt: 1,
                next_retry_millis: 2_000,
            }
        );
    }

    #[test]
    fn the_connectors_own_retry_delay_wins_over_the_guess() {
        assert_eq!(
            connection_status(ConnectionState::Offline {
                reason: "socket closed".to_string(),
                attempt: Some(2),
                // Telegram's stream waits 2 s after its second failure; Messenger would wait 4.
                next_retry_millis: Some(2_000),
            }),
            ConnectionStatus::Reconnecting {
                attempt: 2,
                next_retry_millis: 2_000,
            }
        );
        // A connector that names a delay without counting attempts still gets its number shown.
        assert_eq!(
            connection_status(ConnectionState::Offline {
                reason: "socket closed".to_string(),
                attempt: None,
                next_retry_millis: Some(30_000),
            }),
            ConnectionStatus::Reconnecting {
                attempt: 1,
                next_retry_millis: 30_000,
            }
        );
    }
}
