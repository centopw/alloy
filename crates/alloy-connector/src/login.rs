//! The interactive login state machine.
//!
//! A connector never asks the host for a specific sequence of credentials; it hands back the next
//! [`LoginStep`] and the UI renders whatever that step says. This keeps Telegram's
//! phone → code → (2FA password) flow, Messenger's cookie import, and any future network out of the
//! UI code and out of the engine.
//!
//! Secrets move in [`LoginInput`] only. Their `Debug` output is redacted so a stray log line or
//! panic message cannot leak a password, an OTP or a cookie value.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::types::SessionBlob;

/// A cookie the user has to supply for cookie-based login (Messenger).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookieField {
    /// Cookie name exactly as the browser shows it, e.g. `"c_user"`.
    pub name: String,
    /// Human-readable description for the input label.
    pub label: String,
    /// The connector cannot log in without this field.
    pub required: bool,
}

/// How the network delivered a login code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeChannel {
    /// Delivered inside the network's own app or by an already logged-in session.
    App,
    Sms,
    Email,
    Other,
}

/// Which of the network's own challenges is holding the sign-in.
///
/// A challenge is cleared on the network's own site or app: the connector cannot do it, so it says
/// which one it is and the host guides the user there. The attempt stays open meanwhile (see
/// [`LoginStep::Challenge`]), because the network may accept the answer once the user has cleared it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeKind {
    /// Meta wants this sign-in confirmed on facebook.com or in the Facebook app before the device
    /// may continue.
    Checkpoint,
    /// Meta wants a two-factor code; entering it on the challenge step continues the flow.
    TwoFactor,
    /// Meta wants the account holder to accept a consent screen (new terms, data policy) first.
    Consent,
    /// The account's sign-in can only be completed with a method this client cannot drive: a
    /// passkey, Google OAuth, or an MFA method Meta refuses to route here. Nothing local can clear
    /// it; the user adds a supported method (SMS, authenticator app, password) on Meta's own site,
    /// or imports browser cookies instead.
    UnsupportedMfa,
}

/// Why a login attempt ended. A `Failed` step ends the attempt; the host restarts with
/// [`crate::Connector::begin_login`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginFailure {
    /// The submitted value was missing, malformed or rejected (wrong OTP, wrong password, bad
    /// cookie). `field` names the input, e.g. `"code"` or `"password"`.
    InvalidInput {
        field: Option<String>,
        message: String,
    },
    /// The step the user was answering is no longer valid (QR code or OTP expired).
    Expired,
    /// This connector does not implement the requested login method.
    NotSupported,
    /// The network asked us to slow down before another attempt.
    RateLimited { retry_after_ms: u64 },
    /// Transport failure while logging in.
    Network { message: String },
    /// The network blocked the account itself (suspension, restriction). The user has to resolve it
    /// on the network's own site or app; no local retry helps.
    AccountLocked { message: String },
}

/// The next thing the UI must show. Steps are renderer instructions, not UI state: the host stores
/// nothing about which step came before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginStep {
    /// Ask for a phone number in international format.
    PhoneNumber,
    /// Ask for the login code the network sent.
    Code {
        via: CodeChannel,
        /// Number of characters the network expects, when it says so.
        length: Option<u8>,
    },
    /// Ask for the account's two-step verification password.
    Password {
        /// Network-provided hint, when it has one. Never the password itself.
        hint: Option<String>,
    },
    /// Show a QR code the user scans with the network's mobile app. `payload` is the string to
    /// encode. `expires_at` is milliseconds since the Unix epoch; after that the host calls
    /// [`crate::Connector::submit_login`] with [`LoginInput::QrRefresh`].
    Qr { payload: String, expires_at: i64 },
    /// Ask the user to copy cookies from a logged-in browser session (Messenger).
    Cookies {
        fields: Vec<CookieField>,
        /// Page that explains where to find the cookies, when the network has one.
        help_url: Option<String>,
    },
    /// Ask for a username/handle and password (networks that log in with credentials).
    Credentials {
        /// Label for the username field as the network names it.
        username_label: String,
        /// Other methods this account may use instead, in preference order. Empty when the step
        /// is the only way in; the UI offers a link per entry and answers with
        /// [`LoginInput::Switch`].
        alternatives: Vec<LoginAlternative>,
    },
    /// The network wants this sign-in cleared on its own site or app before this device continues.
    ///
    /// The attempt stays open, so the user clears it there and answers with a code (when `kind` is
    /// [`ChallengeKind::TwoFactor`]), switches to another method, or cancels. Nothing local ends the
    /// attempt: the network's own site is where the challenge lives.
    Challenge {
        kind: ChallengeKind,
        /// The network's own explanation, shown next to the host's copy.
        message: String,
    },
    /// Login finished. The session blob must be persisted before the connector is started.
    Done {
        remote_user_id: String,
        display_name: String,
        session: SessionBlob,
    },
    /// The attempt failed. Terminal for this attempt.
    Failed { reason: LoginFailure },
}

impl LoginStep {
    /// Whether the UI should close the login flow after rendering this step.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Done { .. } | Self::Failed { .. })
    }
}

/// What the user typed for the step the connector last returned.
///
/// Secrets are redacted in `Debug`; see the module docs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginInput {
    /// Answer to [`LoginStep::PhoneNumber`], international format (`+15550100`).
    PhoneNumber { phone: String },
    /// Answer to [`LoginStep::Code`].
    Code { code: String },
    /// Answer to [`LoginStep::Password`].
    Password { password: String },
    /// Answer to [`LoginStep::Cookies`]. Keys are [`CookieField::name`] values.
    Cookies { values: BTreeMap<String, String> },
    /// Answer to [`LoginStep::Credentials`].
    Credentials { username: String, password: String },
    /// One of the [`LoginStep::Credentials::alternatives`] the user picked instead.
    Switch { to: LoginAlternative },
    /// Ask for a fresh [`LoginStep::Qr`] payload (previous one expired, or the user chose QR login).
    QrRefresh,
}

/// A login method offered next to the one currently asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginAlternative {
    /// Copy the cookies of a browser session that is already signed in.
    Cookies,
}

const REDACTED: &str = "<redacted>";

impl fmt::Debug for LoginInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PhoneNumber { phone } => {
                f.debug_struct("PhoneNumber").field("phone", phone).finish()
            }
            Self::Code { .. } => f.debug_struct("Code").field("code", &REDACTED).finish(),
            Self::Password { .. } => f
                .debug_struct("Password")
                .field("password", &REDACTED)
                .finish(),
            Self::Cookies { values } => {
                let redacted: BTreeMap<&str, &str> = values
                    .keys()
                    .map(|name| (name.as_str(), REDACTED))
                    .collect();
                f.debug_struct("Cookies")
                    .field("values", &redacted)
                    .finish()
            }
            Self::Credentials { username, .. } => f
                .debug_struct("Credentials")
                .field("username", username)
                .field("password", &REDACTED)
                .finish(),
            Self::Switch { to } => f.debug_struct("Switch").field("to", to).finish(),
            Self::QrRefresh => f.write_str("QrRefresh"),
        }
    }
}
