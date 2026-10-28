use alloy_engine::context::EngineContext;
use alloy_engine::router::{AppEngine, Engine};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse, SubscriptionId};
use alloy_storage::{InMemorySecretStore, SecretStore};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::tempdir;

fn create_test_engine() -> (AppEngine, tempfile::TempDir) {
    let tmp = tempdir().expect("failed to create temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let ctx = EngineContext::new(tmp.path().to_path_buf(), secrets);
    let engine = AppEngine::new(ctx);
    (engine, tmp)
}

#[tokio::test]
async fn test_routing_by_group() {
    let (engine, _tmp) = create_test_engine();

    // 1. Settings group routes to SettingsHandler
    let res = engine.handle(EngineCommand::ListThemes).await;
    assert!(res.is_ok(), "ListThemes should succeed");
    if let Ok(EngineResponse::ThemesList { themes }) = res {
        assert!(themes.iter().any(|(id, _)| id == "alloy-dark"));
    } else {
        panic!("Unexpected response: {:?}", res);
    }

    // 2. Rooms group routes to RoomsHandler
    let unsub_res = engine
        .handle(EngineCommand::Unsubscribe {
            subscription_id: SubscriptionId("sub_none".into()),
        })
        .await;
    assert!(
        matches!(unsub_res, Ok(EngineResponse::Unsubscribed { .. })),
        "Unsubscribe should route to RoomsHandler"
    );

    // 3. Auth group routes to AuthHandler (no accounts on a fresh data dir)
    let auth_res = engine.handle(EngineCommand::ListAccounts).await;
    assert!(
        matches!(auth_res, Ok(EngineResponse::AccountsList { ref accounts }) if accounts.is_empty()),
        "ListAccounts should route to AuthHandler"
    );
}

#[tokio::test]
async fn test_error_propagation() {
    let (engine, _tmp) = create_test_engine();

    // Send malformed JSON to SetSetting
    let res = engine
        .handle(EngineCommand::SetSetting {
            key: "test_key".into(),
            value_json: "this is not json {".into(),
        })
        .await;

    // Verify error was NOT swallowed or converted to success
    assert!(res.is_err(), "Handler error must propagate");
    let err = res.unwrap_err();
    assert_eq!(err.code, "invalid_argument");
    assert!(err.message.contains("Invalid JSON value"));
}

#[tokio::test]
async fn test_not_implemented_naming() {
    let (engine, _tmp) = create_test_engine();

    let cases = [(
        EngineCommand::StartOidcLogin {
            homeserver_url: "https://matrix.org".into(),
        },
        "StartOidcLogin",
    )];

    for (cmd, expected_name) in cases {
        let res = engine.handle(cmd).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, "not_implemented");
        assert_eq!(
            err.message,
            format!("Command '{}' not yet implemented", expected_name)
        );
    }
}

#[tokio::test]
async fn test_unsubscribe_aborts_spawned_task() {
    let (engine, _tmp) = create_test_engine();
    let ctx = engine.context();

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    // Spawn a long-running background task
    let handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }
    });

    let sub_id = SubscriptionId("sub_cancel_test".into());
    ctx.subscriptions
        .register(sub_id.clone(), handle.abort_handle())
        .await;

    assert!(ctx.subscriptions.is_active(&sub_id).await);

    // Let the task run briefly
    tokio::time::sleep(Duration::from_millis(50)).await;
    let initial_count = counter.load(Ordering::SeqCst);
    assert!(initial_count > 0, "Task should have incremented counter");

    // Unsubscribe via engine command
    let res = engine
        .handle(EngineCommand::Unsubscribe {
            subscription_id: sub_id.clone(),
        })
        .await;

    assert!(matches!(res, Ok(EngineResponse::Unsubscribed { .. })));
    assert!(!ctx.subscriptions.is_active(&sub_id).await);

    // Wait and verify counter does not increment further (task was aborted)
    tokio::time::sleep(Duration::from_millis(50)).await;
    let after_count_1 = counter.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let after_count_2 = counter.load(Ordering::SeqCst);

    assert_eq!(
        after_count_1, after_count_2,
        "Aborted task must not increment counter further"
    );
}

#[tokio::test]
async fn test_event_fan_out() {
    let (engine, _tmp) = create_test_engine();
    let sink = engine.sink();

    let mut rx1 = sink.subscribe();
    let mut rx2 = sink.subscribe();
    let mut rx3 = sink.subscribe();

    assert_eq!(sink.receiver_count(), 3);

    let event = EngineEvent::ToastNotice {
        message: "Fan out test message".into(),
        level: "info".into(),
    };

    sink.emit(event.clone());

    let e1 = rx1.recv().await.expect("rx1 should receive event");
    let e2 = rx2.recv().await.expect("rx2 should receive event");
    let e3 = rx3.recv().await.expect("rx3 should receive event");

    assert_eq!(e1, event);
    assert_eq!(e2, event);
    assert_eq!(e3, event);
}

#[tokio::test]
async fn test_settings_persistence_round_trip() {
    let tmp = tempdir().expect("failed to create temp dir");
    let data_dir = tmp.path().to_path_buf();
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());

    // 1. Initial settings should be empty
    {
        let ctx = EngineContext::new(data_dir.clone(), secrets.clone());
        let engine = AppEngine::new(ctx);

        let res = engine.handle(EngineCommand::GetSettings).await.unwrap();
        if let EngineResponse::SettingsRetrieved { settings_json } = res {
            assert_eq!(settings_json, "{}");
        } else {
            panic!("Expected SettingsRetrieved");
        }

        // Set settings
        engine
            .handle(EngineCommand::SetSetting {
                key: "dark_mode".into(),
                value_json: "true".into(),
            })
            .await
            .unwrap();

        engine
            .handle(EngineCommand::SetSetting {
                key: "font_size".into(),
                value_json: "14".into(),
            })
            .await
            .unwrap();

        let updated = engine.handle(EngineCommand::GetSettings).await.unwrap();
        if let EngineResponse::SettingsRetrieved { settings_json } = updated {
            let val: serde_json::Value = serde_json::from_str(&settings_json).unwrap();
            assert_eq!(val["dark_mode"], true);
            assert_eq!(val["font_size"], 14);
        } else {
            panic!("Expected SettingsRetrieved");
        }
    }

    // 2. Fresh engine instance with same data_dir should read persisted settings
    {
        let ctx = EngineContext::new(data_dir, secrets);
        let engine = AppEngine::new(ctx);

        let res = engine.handle(EngineCommand::GetSettings).await.unwrap();
        if let EngineResponse::SettingsRetrieved { settings_json } = res {
            let val: serde_json::Value = serde_json::from_str(&settings_json).unwrap();
            assert_eq!(val["dark_mode"], true);
            assert_eq!(val["font_size"], 14);
        } else {
            panic!("Expected SettingsRetrieved");
        }
    }
}

#[tokio::test]
async fn test_get_theme_css_output() {
    let (engine, _tmp) = create_test_engine();

    // 1. Built-in alloy-dark
    let res = engine
        .handle(EngineCommand::GetTheme {
            id: "alloy-dark".into(),
        })
        .await
        .unwrap();

    if let EngineResponse::ThemeCss { css } = res {
        assert!(css.starts_with("[data-theme=\"alloy-dark\"] {\n"));
        assert!(css.contains("--alloy-color-surface-canvas:"));
        assert!(css.contains("--alloy-color-text-primary:"));
        assert!(css.ends_with("}\n"));
    } else {
        panic!("Expected ThemeCss response");
    }

    // 2. Built-in alloy-light
    let res_light = engine
        .handle(EngineCommand::GetTheme {
            id: "alloy-light".into(),
        })
        .await
        .unwrap();

    if let EngineResponse::ThemeCss { css } = res_light {
        assert!(css.starts_with("[data-theme=\"alloy-light\"] {\n"));
        assert!(css.contains("--alloy-color-surface-canvas:"));
    } else {
        panic!("Expected ThemeCss response");
    }

    // 3. Nonexistent theme returns not_found
    let err_res = engine
        .handle(EngineCommand::GetTheme {
            id: "does-not-exist".into(),
        })
        .await;

    assert!(err_res.is_err());
    let err = err_res.unwrap_err();
    assert_eq!(err.code, "not_found");

    // 4. SetActiveTheme sets active_theme in settings
    let set_active = engine
        .handle(EngineCommand::SetActiveTheme {
            id: "alloy-dark".into(),
        })
        .await;
    assert!(matches!(set_active, Ok(EngineResponse::ThemeSetActive)));

    let settings = engine.handle(EngineCommand::GetSettings).await.unwrap();
    if let EngineResponse::SettingsRetrieved { settings_json } = settings {
        let val: serde_json::Value = serde_json::from_str(&settings_json).unwrap();
        assert_eq!(val["active_theme"], "alloy-dark");
    } else {
        panic!("Expected SettingsRetrieved");
    }
}

#[tokio::test]
async fn test_unreadable_settings_are_reported_and_never_overwritten() {
    let tmp = tempdir().expect("failed to create temp dir");
    let settings_path = tmp.path().join("settings.json");
    // What a truncated write leaves behind: the user's settings are unreadable, not empty.
    let corrupt = "{ \"prefs\": { \"fontScale\": ";
    std::fs::write(&settings_path, corrupt).unwrap();

    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let ctx = EngineContext::new(tmp.path().to_path_buf(), secrets);
    let engine = AppEngine::new(ctx);

    // A failed read is an error, not an empty object: the UI keeps its local copy instead of
    // pushing defaults over settings it could not read.
    let err = engine
        .handle(EngineCommand::GetSettings)
        .await
        .expect_err("reading corrupt settings must fail");
    assert_eq!(err.code, "internal_error");

    // A write also fails, so the unreadable file is left exactly as it was.
    let err = engine
        .handle(EngineCommand::SetSetting {
            key: "prefs".into(),
            value_json: "{}".into(),
        })
        .await
        .expect_err("writing over corrupt settings must fail");
    assert_eq!(err.code, "internal_error");
    assert_eq!(std::fs::read_to_string(&settings_path).unwrap(), corrupt);
}

#[tokio::test]
async fn test_a_failed_settings_write_never_replaces_the_readable_file() {
    let tmp = tempdir().expect("failed to create temp dir");
    let settings_path = tmp.path().join("settings.json");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let engine = AppEngine::new(EngineContext::new(tmp.path().to_path_buf(), secrets));

    // A successful write leaves the readable file and no temp file behind.
    engine
        .handle(EngineCommand::SetSetting {
            key: "active_theme".into(),
            value_json: "\"alloy-dark\"".into(),
        })
        .await
        .unwrap();
    assert!(!tmp.path().join("settings.json.tmp").exists());
    let readable = std::fs::read_to_string(&settings_path).unwrap();

    // Make the temp path unusable: the write must fail before the rename, so the readable file
    // and its contents are untouched.
    std::fs::create_dir(tmp.path().join("settings.json.tmp")).unwrap();
    let err = engine
        .handle(EngineCommand::SetSetting {
            key: "prefs".into(),
            value_json: "{}".into(),
        })
        .await
        .expect_err("a write that cannot stage its temp file must fail");
    assert_eq!(err.code, "internal_error");

    assert_eq!(std::fs::read_to_string(&settings_path).unwrap(), readable);
    let parsed: serde_json::Value = serde_json::from_str(&readable).unwrap();
    assert_eq!(parsed["active_theme"], "alloy-dark");
}
