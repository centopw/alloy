//! # alloy-ffi
//!
//! Android JNI exports for headless background push notification handling.
//!
//! # Unsafe code policy
//!
//! This crate is the only crate in the Alloy workspace permitted to use `unsafe` code.
//! JNI exported functions (`#[unsafe(no_mangle)] pub extern "system" fn Java_...`)
//! necessarily use the C ABI and interact with the JVM via `JNIEnv` raw pointers.
//!
//! All exported functions wrap the real implementation in `std::panic::catch_unwind`
//! and return errors as JSON strings to avoid panics crossing the JNI boundary.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::LazyLock;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime for background push")
});

fn error_to_jstring(env: &mut JNIEnv, err_json: &str) -> jstring {
    env.new_string(err_json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Fetch a notification by room_id and event_id from the homeserver.
///
/// JNI signature: `(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;`
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_alloy_PushNative_fetchNotification<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    data_dir: JString<'local>,
    payload_json: JString<'local>,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let data_dir_str: String = match env.get_string(&data_dir) {
            Ok(s) => s.into(),
            Err(e) => {
                return serde_json::json!({
                    "status": "error",
                    "error": format!("invalid data_dir string: {e}")
                })
                .to_string();
            }
        };

        let payload_str: String = match env.get_string(&payload_json) {
            Ok(s) => s.into(),
            Err(e) => {
                return serde_json::json!({
                    "status": "error",
                    "error": format!("invalid payload_json string: {e}")
                })
                .to_string();
            }
        };

        let data_path = std::path::Path::new(&data_dir_str);
        let value = RUNTIME.block_on(alloy_push::fetch_notification(data_path, &payload_str));
        value.to_string()
    }));

    match result {
        Ok(json_str) => match env.new_string(&json_str) {
            Ok(js) => js.into_raw(),
            Err(e) => error_to_jstring(
                &mut env,
                &serde_json::json!({
                    "status": "error",
                    "error": format!("failed to allocate JNI string: {e}")
                })
                .to_string(),
            ),
        },
        Err(_) => error_to_jstring(
            &mut env,
            &serde_json::json!({
                "status": "error",
                "error": "panic inside fetchNotification"
            })
            .to_string(),
        ),
    }
}

/// Register an HTTP pusher with the homeserver.
///
/// JNI signature: `(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;`
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_alloy_PushNative_setPusher<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    data_dir: JString<'local>,
    config_json: JString<'local>,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let data_dir_str: String = match env.get_string(&data_dir) {
            Ok(s) => s.into(),
            Err(e) => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("invalid data_dir string: {e}")
                })
                .to_string();
            }
        };

        let config_str: String = match env.get_string(&config_json) {
            Ok(s) => s.into(),
            Err(e) => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("invalid config_json string: {e}")
                })
                .to_string();
            }
        };

        let data_path = std::path::Path::new(&data_dir_str);
        let value = RUNTIME.block_on(alloy_push::set_pusher(data_path, &config_str));
        value.to_string()
    }));

    match result {
        Ok(json_str) => match env.new_string(&json_str) {
            Ok(js) => js.into_raw(),
            Err(e) => error_to_jstring(
                &mut env,
                &serde_json::json!({
                    "ok": false,
                    "error": format!("failed to allocate JNI string: {e}")
                })
                .to_string(),
            ),
        },
        Err(_) => error_to_jstring(
            &mut env,
            &serde_json::json!({
                "ok": false,
                "error": "panic inside setPusher"
            })
            .to_string(),
        ),
    }
}

/// Remove a pusher from the homeserver.
///
/// JNI signature: `(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;`
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_alloy_PushNative_removePusher<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    data_dir: JString<'local>,
    config_json: JString<'local>,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let data_dir_str: String = match env.get_string(&data_dir) {
            Ok(s) => s.into(),
            Err(e) => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("invalid data_dir string: {e}")
                })
                .to_string();
            }
        };

        let config_str: String = match env.get_string(&config_json) {
            Ok(s) => s.into(),
            Err(e) => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("invalid config_json string: {e}")
                })
                .to_string();
            }
        };

        let data_path = std::path::Path::new(&data_dir_str);
        let value = RUNTIME.block_on(alloy_push::remove_pusher(data_path, &config_str));
        value.to_string()
    }));

    match result {
        Ok(json_str) => match env.new_string(&json_str) {
            Ok(js) => js.into_raw(),
            Err(e) => error_to_jstring(
                &mut env,
                &serde_json::json!({
                    "ok": false,
                    "error": format!("failed to allocate JNI string: {e}")
                })
                .to_string(),
            ),
        },
        Err(_) => error_to_jstring(
            &mut env,
            &serde_json::json!({
                "ok": false,
                "error": "panic inside removePusher"
            })
            .to_string(),
        ),
    }
}
