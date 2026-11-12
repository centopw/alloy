//! The login state machine’s private state.
//!
//! The step values themselves live in `alloy_connector`; this module only holds what has to
//! survive between two `submit_login` calls (the code token, the 2FA token, the QR token) and the
//! two pure helpers the connector needs: the QR payload spelling and which field an
//! out-of-order input belongs to.

use alloy_connector::LoginInput;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use grammers_client::client::PasswordToken;

/// How many datacenter migrations one QR export may chain through before giving up.
///
/// Telegram migrates at most once in practice; the bound only exists so a misbehaving server cannot
/// loop us forever inside one `submit_login`.
pub(crate) const MAX_QR_MIGRATIONS: usize = 3;

/// What the current login attempt is waiting for.
pub(crate) enum LoginFlow {
    /// Waiting for a phone number (or a QR refresh, if the user prefers that).
    Phone,
    /// Waiting for the code Telegram sent, with the token `auth.signIn` needs.
    Code {
        /// The number the code was sent to.
        phone: String,
        /// `phone_code_hash` from `auth.sendCode`.
        phone_code_hash: String,
    },
    /// Waiting for the two-step verification password, with the SRP token to check it.
    Password {
        /// Token built from `account.getPassword`.
        token: PasswordToken,
    },
    /// Waiting for the user to scan the code with an official app.
    ///
    /// The token itself is not kept: a refresh always re-exports one, which is also where Telegram
    /// reports that the code was scanned.
    Qr,
}

/// The string to encode in the QR image.
pub(crate) fn qr_payload(token: &[u8]) -> String {
    format!("tg://login?token={}", URL_SAFE_NO_PAD.encode(token))
}

/// The login field an input belongs to, for `LoginFailure::InvalidInput`.
pub(crate) fn field_of(input: &LoginInput) -> &'static str {
    match input {
        LoginInput::PhoneNumber { .. } => "phone",
        LoginInput::Code { .. } => "code",
        LoginInput::Password { .. } => "password",
        LoginInput::Cookies { .. } => "cookies",
        LoginInput::Credentials { .. } => "credentials",
        LoginInput::QrRefresh => "qr",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_payloads_are_url_safe_and_unpadded() {
        let payload = qr_payload(&[0xfb, 0xff, 0x00, 0x01]);

        assert_eq!(payload, "tg://login?token=-_8AAQ");
        let token = payload.trim_start_matches("tg://login?token=");
        assert!(!token.contains('='), "no padding: {payload}");
        assert!(!token.contains('+'), "url-safe alphabet: {payload}");
        assert!(!token.contains('/'), "url-safe alphabet: {payload}");
    }

    #[test]
    fn inputs_name_their_field() {
        assert_eq!(
            field_of(&LoginInput::PhoneNumber {
                phone: "+1".to_string()
            }),
            "phone"
        );
        assert_eq!(
            field_of(&LoginInput::Code {
                code: "1".to_string()
            }),
            "code"
        );
        assert_eq!(
            field_of(&LoginInput::Password {
                password: "x".to_string()
            }),
            "password"
        );
        assert_eq!(field_of(&LoginInput::QrRefresh), "qr");
    }
}
