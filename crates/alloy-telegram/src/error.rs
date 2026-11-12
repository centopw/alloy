//! Translation of grammers failures into the connector contract.
//!
//! Everything a caller sees is either [`ConnectorError`] (data calls) or [`LoginFailure`] (login
//! steps). Both are derived here, so the classification lives in one place:
//!
//! | grammers | connector |
//! |---|---|
//! | `FLOOD_WAIT_X` / `FLOOD_PREMIUM_WAIT_X` | `RateLimited { retry_after_ms: X * 1000 }` |
//! | `AUTH_KEY_UNREGISTERED`, `SESSION_REVOKED`, … , HTTP 401 | `Unauthorized` |
//! | 400 / 403 | `InvalidInput` (the request cannot succeed as sent) |
//! | I/O, transport, dropped connection, 5xx | `Network` (retryable) |
//! | `MESSAGE_EDIT_TIME_EXPIRED` | `NotSupported { feature: "edit" }` |
//!
//! During login the same RPC errors become [`LoginFailure`]: bad input names the offending field,
//! an expired code is [`LoginFailure::Expired`], a restricted account is
//! [`LoginFailure::AccountLocked`], floods and transport failures keep their retry semantics.

use alloy_connector::{ConnectorError, LoginFailure};
use grammers_mtsender::{InvocationError, RpcError};

/// Flood-wait error names. Telegram appends the seconds to the name and strips them into `value`.
const FLOOD_WAIT: &str = "FLOOD_WAIT";
const FLOOD_PREMIUM_WAIT: &str = "FLOOD_PREMIUM_WAIT";

/// How long Telegram asked us to wait, in milliseconds.
///
/// Returns `None` for every other error. `FLOOD_WAIT` without a number is treated as one second,
/// so a caller always gets a usable delay.
pub(crate) fn rate_limit(error: &RpcError) -> Option<u64> {
    if error.is(FLOOD_WAIT) || error.is(FLOOD_PREMIUM_WAIT) {
        let seconds = u64::from(error.value.unwrap_or(1));
        Some(seconds.saturating_mul(1_000).max(1_000))
    } else {
        None
    }
}

/// Whether the stored authorization key is dead and only a new login helps.
pub(crate) fn is_session_dead(error: &RpcError) -> bool {
    matches!(
        error.name.as_str(),
        "AUTH_KEY_UNREGISTERED"
            | "AUTH_KEY_INVALID"
            | "AUTH_KEY_DUPLICATED"
            | "SESSION_REVOKED"
            | "SESSION_EXPIRED"
    ) || error.code == 401
}

/// Whether a failed request means the stored session is dead.
pub(crate) fn is_dead_session(error: &InvocationError) -> bool {
    matches!(error, InvocationError::Rpc(rpc) if is_session_dead(rpc))
}

/// Maps a failed request on an established session.
pub(crate) fn map_invocation(error: InvocationError) -> ConnectorError {
    match error {
        InvocationError::Rpc(rpc) => map_rpc(&rpc),
        other => ConnectorError::Network {
            message: other.to_string(),
        },
    }
}

/// Maps an RPC error on an established session.
fn map_rpc(error: &RpcError) -> ConnectorError {
    if let Some(retry_after_ms) = rate_limit(error) {
        return ConnectorError::RateLimited { retry_after_ms };
    }
    if is_session_dead(error) {
        return ConnectorError::Unauthorized;
    }
    if error.is("MESSAGE_EDIT_TIME_EXPIRED") {
        return ConnectorError::NotSupported {
            feature: "edit".to_string(),
        };
    }
    let message = format!("{} {}", error.code, error.name);
    match error.code {
        400 | 403 => ConnectorError::InvalidInput { message },
        // 5xx is a server-side hiccup and is worth retrying; everything else (404, 406, …) is
        // reported as a transport-level problem so the host backs off instead of dropping data.
        _ => ConnectorError::Network { message },
    }
}

/// Maps a failed login request. `field` names the input the user just submitted.
pub(crate) fn map_login(error: InvocationError, field: &str) -> LoginFailure {
    match error {
        InvocationError::Rpc(rpc) => map_login_rpc(&rpc, field),
        other => LoginFailure::Network {
            message: other.to_string(),
        },
    }
}

/// Maps an RPC error raised during login.
fn map_login_rpc(error: &RpcError, field: &str) -> LoginFailure {
    if let Some(retry_after_ms) = rate_limit(error) {
        return LoginFailure::RateLimited { retry_after_ms };
    }
    match error.name.as_str() {
        "PHONE_CODE_EXPIRED"
        | "PHONE_CODE_HASH_EMPTY"
        | "EMAIL_CODE_EXPIRED"
        | "EMAIL_VERIFY_EXPIRED" => LoginFailure::Expired,
        "PHONE_NUMBER_UNOCCUPIED" => LoginFailure::InvalidInput {
            field: Some(field.to_string()),
            message: "this number has no Telegram account; create one in an official app first"
                .to_string(),
        },
        "USER_DEACTIVATED" | "USER_DEACTIVATED_BAN" | "PHONE_NUMBER_BANNED" => {
            LoginFailure::AccountLocked {
                message: "Telegram restricted this account; resolve it in an official app"
                    .to_string(),
            }
        }
        "PHONE_NUMBER_INVALID"
        | "PHONE_NUMBER_FLOOD"
        | "PHONE_CODE_INVALID"
        | "PHONE_CODE_EMPTY"
        | "PASSWORD_HASH_INVALID"
        | "PHONE_PASSWORD_INVALID"
        | "EMAIL_INVALID"
        | "EMAIL_UNCONFIRMED" => LoginFailure::InvalidInput {
            field: Some(field.to_string()),
            message: format!("Telegram rejected the {field}: {}", error.name),
        },
        _ => LoginFailure::Network {
            message: format!("{} {}", error.code, error.name),
        },
    }
}

/// A login failure naming `field` with a fixed message.
pub(crate) fn invalid_input(field: &str, message: impl Into<String>) -> LoginFailure {
    LoginFailure::InvalidInput {
        field: Some(field.to_string()),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpc(code: i32, name: &str, value: Option<u32>) -> InvocationError {
        InvocationError::Rpc(RpcError {
            code,
            name: name.to_string(),
            value,
            caused_by: None,
        })
    }

    #[test]
    fn flood_wait_becomes_a_retry_delay() {
        let error = map_invocation(rpc(420, FLOOD_WAIT, Some(7)));

        assert_eq!(
            error,
            ConnectorError::RateLimited {
                retry_after_ms: 7_000
            }
        );
        assert!(error.is_retryable());
    }

    #[test]
    fn premium_flood_wait_without_a_value_uses_one_second() {
        let error = map_invocation(rpc(420, FLOOD_PREMIUM_WAIT, None));

        assert_eq!(
            error,
            ConnectorError::RateLimited {
                retry_after_ms: 1_000
            }
        );
    }

    #[test]
    fn dead_session_is_unauthorized() {
        assert_eq!(
            map_invocation(rpc(401, "AUTH_KEY_UNREGISTERED", None)),
            ConnectorError::Unauthorized
        );
        assert_eq!(
            map_invocation(rpc(401, "SOMETHING_NEW", None)),
            ConnectorError::Unauthorized
        );
        assert_eq!(
            map_invocation(rpc(400, "SESSION_REVOKED", None)),
            ConnectorError::Unauthorized
        );
    }

    #[test]
    fn bad_request_is_invalid_input_and_names_the_rpc_error() {
        assert_eq!(
            map_invocation(rpc(400, "MESSAGE_ID_INVALID", None)),
            ConnectorError::InvalidInput {
                message: "400 MESSAGE_ID_INVALID".to_string()
            }
        );
        assert_eq!(
            map_invocation(rpc(403, "CHAT_WRITE_FORBIDDEN", None)),
            ConnectorError::InvalidInput {
                message: "403 CHAT_WRITE_FORBIDDEN".to_string()
            }
        );
    }

    #[test]
    fn expired_edit_is_not_supported() {
        assert_eq!(
            map_invocation(rpc(400, "MESSAGE_EDIT_TIME_EXPIRED", None)),
            ConnectorError::NotSupported {
                feature: "edit".to_string()
            }
        );
    }

    #[test]
    fn transport_failures_are_retryable_network_errors() {
        let error = map_invocation(InvocationError::Dropped("pool went away"));

        assert!(matches!(error, ConnectorError::Network { .. }));
        assert!(error.is_retryable());
        assert_eq!(
            map_invocation(rpc(500, "INTERNAL", None)),
            ConnectorError::Network {
                message: "500 INTERNAL".to_string()
            }
        );
    }

    #[test]
    fn login_errors_name_the_submitted_field() {
        assert_eq!(
            map_login(rpc(400, "PHONE_CODE_INVALID", None), "code"),
            LoginFailure::InvalidInput {
                field: Some("code".to_string()),
                message: "Telegram rejected the code: PHONE_CODE_INVALID".to_string()
            }
        );
        assert_eq!(
            map_login(rpc(400, "PHONE_CODE_EXPIRED", None), "code"),
            LoginFailure::Expired
        );
        assert_eq!(
            map_login(rpc(420, FLOOD_WAIT, Some(3)), "phone"),
            LoginFailure::RateLimited {
                retry_after_ms: 3_000
            }
        );
        assert!(matches!(
            map_login(InvocationError::Dropped("offline"), "code"),
            LoginFailure::Network { .. }
        ));
        assert!(matches!(
            map_login(rpc(401, "USER_DEACTIVATED_BAN", None), "phone"),
            LoginFailure::AccountLocked { .. }
        ));
    }

    #[test]
    fn unregistered_number_points_at_the_official_app() {
        let failure = map_login(rpc(400, "PHONE_NUMBER_UNOCCUPIED", None), "phone");

        let LoginFailure::InvalidInput { field, message } = failure else {
            panic!("expected invalid input");
        };
        assert_eq!(field.as_deref(), Some("phone"));
        assert!(message.contains("official app"), "{message}");
    }
}
