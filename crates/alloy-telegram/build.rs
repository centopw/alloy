//! Bakes Alloy's registered Telegram application into the binary.
//!
//! `api_id`/`api_hash` identify the *application*, exactly the role they play in a mautrix bridge
//! config: whoever builds sets them once, and the person using the installed app never sees them.
//! Reading them here means an installed copy needs no environment at all.
//!
//! A build without the pair still succeeds (empty strings are baked, which
//! [`alloy_telegram::TelegramConfig::resolve`] treats as unset): `cargo build` on a fresh clone
//! works and simply reports Telegram as unavailable, with the registration URL in the log.
//!
//! The variable names are duplicated from `src/config.rs` because a build script cannot use the
//! crate it builds.

use std::env;

/// Environment variable carrying the Telegram `api_id` at build time.
const API_ID_ENV: &str = "ALLOY_TELEGRAM_API_ID";
/// Environment variable carrying the Telegram `api_hash` at build time.
const API_HASH_ENV: &str = "ALLOY_TELEGRAM_API_HASH";

fn main() {
    println!("cargo:rerun-if-env-changed={API_ID_ENV}");
    println!("cargo:rerun-if-env-changed={API_HASH_ENV}");
    let api_id = env::var(API_ID_ENV).unwrap_or_default();
    let api_hash = env::var(API_HASH_ENV).unwrap_or_default();
    println!("cargo:rustc-env=ALLOY_TELEGRAM_APP_ID={}", api_id.trim());
    println!(
        "cargo:rustc-env=ALLOY_TELEGRAM_APP_HASH={}",
        api_hash.trim()
    );
}
