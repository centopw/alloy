//! Voice messages, static location, and live location against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, RoomId, SubscriptionId, TimelineItemContent,
    TimelineItemDto, VectorDiffDto,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);

/// A minimal OGG Vorbis voice file.
const VOICE_OGG: &[u8] = &[
    0x4f, 0x67, 0x67, 0x53, 0x00, 0x02, 0x00, 0x00, // OggS header
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Checksum placeholder
    0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, // Sequence
];

fn engine(dir: &tempfile::TempDir) -> (AppEngine, EngineContext) {
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    (eng, ctx)
}

async fn login(eng: &AppEngine, hs: &str, username: &str) {
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs.to_owned(),
        username: username.into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();
    eng.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
}

fn collect(list: &mut Vec<TimelineItemDto>, diff: VectorDiffDto<TimelineItemDto>) {
    match diff {
        VectorDiffDto::Append { values } => list.extend(values),
        VectorDiffDto::PushBack { value } => list.push(value),
        VectorDiffDto::PushFront { value } => list.insert(0, value),
        VectorDiffDto::Insert { index, value } => list.insert(index, value),
        VectorDiffDto::Set { index, value } => list[index] = value,
        VectorDiffDto::Remove { index } => {
            list.remove(index);
        }
        VectorDiffDto::Reset { values } => *list = values,
        VectorDiffDto::Clear => list.clear(),
        VectorDiffDto::PopBack => {
            list.pop();
        }
        VectorDiffDto::PopFront => {
            list.remove(0);
        }
        VectorDiffDto::Truncate { length } => list.truncate(length),
    }
}

#[tokio::test]
async fn voice_message_send_and_readback() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            eprintln!("Skipping: ALLOY_TEST_HS not set");
            return;
        }
    };

    let dir = tempfile::TempDir::new().unwrap();
    let (eng, ctx) = engine(&dir);

    login(&eng, &hs, "alice").await;

    // Create a room
    let room_id: RoomId = match eng
        .handle(EngineCommand::CreateRoom {
            name: "voice-test".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let mut rx = ctx.sink.subscribe();

    let mut tries = 0;
    let sub: SubscriptionId = loop {
        match eng
            .handle(EngineCommand::SubscribeTimeline {
                room_id: room_id.clone(),
            })
            .await
        {
            Ok(EngineResponse::TimelineSubscribed { subscription_id }) => break subscription_id,
            Err(e) if e.code == "not_found" && tries < 40 => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            other => panic!("unexpected {other:?}"),
        }
    };

    // Write voice file
    let voice_file = dir.path().join("voice.ogg");
    std::fs::write(&voice_file, VOICE_OGG).unwrap();

    // Verify un-allowed attachment path is rejected
    let unallowed_err = eng
        .handle(EngineCommand::SendVoice {
            room_id: room_id.clone(),
            file_path: voice_file.to_string_lossy().into_owned(),
            duration_ms: 3500,
            waveform: vec![100, 200, 300, 400, 500, 400, 300, 200, 100],
            txn_id: None,
        })
        .await;
    assert!(
        unallowed_err.is_err(),
        "unregistered attachment path should be rejected"
    );
    assert_eq!(unallowed_err.unwrap_err().code, "forbidden");

    // Allow path and send
    ctx.allow_attachment_path(&voice_file).unwrap();

    let send_res = eng
        .handle(EngineCommand::SendVoice {
            room_id: room_id.clone(),
            file_path: voice_file.to_string_lossy().into_owned(),
            duration_ms: 3500,
            waveform: vec![100, 200, 300, 400, 500, 400, 300, 200, 100],
            txn_id: None,
        })
        .await;
    assert!(send_res.is_ok(), "SendVoice failed: {:?}", send_res);

    // Wait for voice message in timeline
    let mut mirror: Vec<TimelineItemDto> = vec![];
    let res = tokio::time::timeout(WAIT, async {
        loop {
            if let EngineEvent::TimelineDiff {
                subscription_id,
                diff,
                ..
            } = rx.recv().await.unwrap()
                && subscription_id == sub
            {
                collect(&mut mirror, *diff);
                if mirror
                    .iter()
                    .any(|i| matches!(&i.content, TimelineItemContent::Media { voice: true, .. }))
                {
                    return;
                }
            }
        }
    })
    .await;
    assert!(res.is_ok(), "voice message never appeared in timeline");
}

#[tokio::test]
async fn static_location_send() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            eprintln!("Skipping: ALLOY_TEST_HS not set");
            return;
        }
    };

    let dir = tempfile::TempDir::new().unwrap();
    let (eng, ctx) = engine(&dir);

    login(&eng, &hs, "alice").await;

    let room_id: RoomId = match eng
        .handle(EngineCommand::CreateRoom {
            name: "location-test".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let mut rx = ctx.sink.subscribe();

    let mut tries = 0;
    let sub: SubscriptionId = loop {
        match eng
            .handle(EngineCommand::SubscribeTimeline {
                room_id: room_id.clone(),
            })
            .await
        {
            Ok(EngineResponse::TimelineSubscribed { subscription_id }) => break subscription_id,
            Err(e) if e.code == "not_found" && tries < 40 => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            other => panic!("unexpected {other:?}"),
        }
    };

    // Send static location
    let resp = eng
        .handle(EngineCommand::SendLocation {
            room_id: room_id.clone(),
            latitude: 37.7749,
            longitude: -122.4194,
            accuracy_m: Some(10.5),
            description: Some("San Francisco".into()),
        })
        .await;

    assert!(resp.is_ok(), "SendLocation failed: {:?}", resp);

    // Wait for location in timeline
    let mut mirror: Vec<TimelineItemDto> = vec![];
    let res = tokio::time::timeout(WAIT, async {
        loop {
            if let EngineEvent::TimelineDiff {
                subscription_id,
                diff,
                ..
            } = rx.recv().await.unwrap()
                && subscription_id == sub
            {
                collect(&mut mirror, *diff);
                if mirror.iter().any(|i| {
                    matches!(
                        &i.content,
                        TimelineItemContent::Location {
                            geo_uri,
                            description,
                            live: false,
                            ..
                        } if geo_uri.starts_with("geo:37.7749,-122.4194") && description.as_deref() == Some("San Francisco")
                    )
                }) {
                    return;
                }
            }
        }
    })
    .await;
    assert!(res.is_ok(), "location message never appeared in timeline");
}

#[tokio::test]
async fn location_validation() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            eprintln!("Skipping: ALLOY_TEST_HS not set");
            return;
        }
    };

    let dir = tempfile::TempDir::new().unwrap();
    let (eng, _ctx) = engine(&dir);

    login(&eng, &hs, "alice").await;

    let room_id: RoomId = match eng
        .handle(EngineCommand::CreateRoom {
            name: "location-validation".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    // Test invalid latitude (> 90)
    let resp = eng
        .handle(EngineCommand::SendLocation {
            room_id: room_id.clone(),
            latitude: 95.0, // Invalid
            longitude: 0.0,
            accuracy_m: None,
            description: None,
        })
        .await;
    assert!(resp.is_err(), "Should reject invalid latitude > 90");

    // Test invalid longitude (> 180)
    let resp = eng
        .handle(EngineCommand::SendLocation {
            room_id: room_id.clone(),
            latitude: 0.0,
            longitude: 200.0, // Invalid
            accuracy_m: None,
            description: None,
        })
        .await;
    assert!(resp.is_err(), "Should reject invalid longitude > 180");

    // Test negative accuracy
    let resp = eng
        .handle(EngineCommand::SendLocation {
            room_id: room_id.clone(),
            latitude: 0.0,
            longitude: 0.0,
            accuracy_m: Some(-5.0), // Invalid
            description: None,
        })
        .await;
    assert!(resp.is_err(), "Should reject negative accuracy");
}

#[tokio::test]
async fn live_location_lifecycle() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            eprintln!("Skipping: ALLOY_TEST_HS not set");
            return;
        }
    };

    let dir = tempfile::TempDir::new().unwrap();
    let (eng, _ctx) = engine(&dir);

    login(&eng, &hs, "alice").await;

    let room_id: RoomId = match eng
        .handle(EngineCommand::CreateRoom {
            name: "live-location-test".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    // Start live location (15 minutes)
    let start_res = eng
        .handle(EngineCommand::StartLiveLocation {
            room_id: room_id.clone(),
            duration_ms: 15 * 60 * 1000,
        })
        .await;
    assert!(
        start_res.is_ok(),
        "StartLiveLocation failed: {:?}",
        start_res
    );

    // Send a beacon update
    let beacon_res = eng
        .handle(EngineCommand::SendLocationBeacon {
            room_id: room_id.clone(),
            geo_uri: "geo:37.7749,-122.4194;u=5".into(),
        })
        .await;
    assert!(
        beacon_res.is_ok(),
        "SendLocationBeacon failed: {:?}",
        beacon_res
    );

    // Stop live location
    let stop_res = eng
        .handle(EngineCommand::StopLiveLocation {
            room_id: room_id.clone(),
        })
        .await;
    assert!(stop_res.is_ok(), "StopLiveLocation failed: {:?}", stop_res);
}
