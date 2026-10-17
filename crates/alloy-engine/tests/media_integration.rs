//! Media upload/download (plain + end-to-end encrypted) against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, MediaSource, RoomId, SubscriptionId,
    TimelineItemContent, TimelineItemDto, VectorDiffDto,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;

/// A valid 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f,
    0x00, 0x05, 0xfe, 0x02, 0xfe, 0xa7, 0x35, 0x81, 0x84, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

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

async fn roundtrip(eng: &AppEngine, ctx: &EngineContext, encrypted: bool, dir: &std::path::Path) {
    let room_id: RoomId = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("media-{encrypted}"),
            encrypted,
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

    let file = dir.join(format!("pic-{encrypted}.png"));
    std::fs::write(&file, PNG).unwrap();
    eng.handle(EngineCommand::SendAttachment {
        room_id: room_id.clone(),
        file_path: file.to_string_lossy().into_owned(),
        mime_type: Some("image/png".into()),
        txn_id: None,
    })
    .await
    .unwrap();

    let mut mirror: Vec<TimelineItemDto> = vec![];
    let (uri, kind) = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let EngineEvent::TimelineDiff {
                subscription_id,
                diff,
                ..
            } = rx.recv().await.unwrap()
                && subscription_id == sub
            {
                collect(&mut mirror, *diff);
                if let Some((uri, kind)) = mirror.iter().find_map(|i| match &i.content {
                    TimelineItemContent::Media {
                        uri, media_kind, ..
                    } if matches!(i.send_state, alloy_protocol::EventSendState::Sent { .. }) => {
                        Some((uri.clone(), media_kind.clone()))
                    }
                    _ => None,
                }) {
                    return (uri, kind);
                }
            }
        }
    })
    .await
    .expect("attachment never appeared as sent media");
    assert_eq!(kind, "image");
    assert_eq!(
        uri.starts_with('{'),
        encrypted,
        "encrypted rooms carry a JSON source: {uri}"
    );

    // Full file: bytes round-trip (decrypted when the room is encrypted).
    let path = match eng
        .handle(EngineCommand::GetMedia {
            source: MediaSource::Remote { uri: uri.clone() },
            kind: "file".into(),
            width: None,
            height: None,
            mime_type: Some("image/png".into()),
        })
        .await
        .unwrap()
    {
        EngineResponse::MediaRetrieved { local_path } => local_path,
        other => panic!("unexpected {other:?}"),
    };
    assert!(path.ends_with(".png"), "{path}");
    assert_eq!(std::fs::read(&path).unwrap(), PNG);

    // Second request is served from the cache (same path).
    let again = match eng
        .handle(EngineCommand::GetMedia {
            source: MediaSource::Remote { uri: uri.clone() },
            kind: "file".into(),
            width: None,
            height: None,
            mime_type: Some("image/png".into()),
        })
        .await
        .unwrap()
    {
        EngineResponse::MediaRetrieved { local_path } => local_path,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(again, path);

    // Thumbnails: server-side for plain media, full-file fallback for encrypted media.
    let thumb = match eng
        .handle(EngineCommand::GetMedia {
            source: MediaSource::Remote { uri },
            kind: "thumbnail".into(),
            width: Some(32),
            height: Some(32),
            mime_type: Some("image/png".into()),
        })
        .await
        .unwrap()
    {
        EngineResponse::MediaRetrieved { local_path } => local_path,
        other => panic!("unexpected {other:?}"),
    };
    assert!(std::fs::metadata(&thumb).unwrap().len() > 0);
}

#[tokio::test]
async fn attachments_roundtrip_plain_and_encrypted() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs,
        username: "alice".into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();

    roundtrip(&eng, &ctx, false, dir.path()).await;
    roundtrip(&eng, &ctx, true, dir.path()).await;

    // Argument validation and missing files are typed errors.
    for (cmd, code) in [
        (
            EngineCommand::GetMedia {
                source: MediaSource::Remote {
                    uri: "mxc://x/y".into(),
                },
                kind: "thumbnail".into(),
                width: None,
                height: None,
                mime_type: None,
            },
            "invalid_argument",
        ),
        (
            EngineCommand::GetMedia {
                source: MediaSource::Local {
                    file_path: "/definitely/missing".into(),
                },
                kind: "file".into(),
                width: None,
                height: None,
                mime_type: None,
            },
            "not_found",
        ),
    ] {
        assert_eq!(eng.handle(cmd).await.unwrap_err().code, code);
    }
}
