//! Room media, files and links listings against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineResponse, RoomId, RoomMediaItem, RoomMediaKind};
use alloy_storage::InMemorySecretStore;
use std::path::Path;
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

async fn listing(
    eng: &AppEngine,
    room_id: &RoomId,
    kind: RoomMediaKind,
    from: Option<String>,
    limit: u32,
) -> (Vec<RoomMediaItem>, Option<String>) {
    match eng
        .handle(EngineCommand::GetRoomMedia {
            room_id: room_id.clone(),
            kind,
            from,
            limit,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomMedia {
            items, next_batch, ..
        } => (items, next_batch),
        other => panic!("unexpected {other:?}"),
    }
}

async fn send_attachment(eng: &AppEngine, room_id: &RoomId, path: &Path, mime: &str) {
    eng.handle(EngineCommand::SendAttachment {
        room_id: room_id.clone(),
        file_path: path.to_string_lossy().into_owned(),
        mime_type: Some(mime.into()),
        txn_id: None,
    })
    .await
    .unwrap();
}

async fn send_text(eng: &AppEngine, room_id: &RoomId, body: &str) {
    eng.handle(EngineCommand::SendText {
        room_id: room_id.clone(),
        body: body.into(),
        markdown: None,
        reply_to: None,
        mentions: None,
        txn_id: None,
        in_thread_root: None,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn room_media_lists_media_files_links_and_pages() {
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

    let room_id: RoomId = match eng
        .handle(EngineCommand::CreateRoom {
            name: "room-media".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    // The freshly created room becomes known to the client after a short sync delay.
    let mut tries = 0;
    loop {
        match eng
            .handle(EngineCommand::SubscribeTimeline {
                room_id: room_id.clone(),
            })
            .await
        {
            Ok(_) => break,
            Err(e) if e.code == "not_found" && tries < 40 => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => panic!("unexpected {e:?}"),
        }
    }

    // Oldest to newest: one file, two images, one link message, one plain message.
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, b"meeting notes").unwrap();
    ctx.allow_attachment_path(&notes).unwrap();
    send_attachment(&eng, &room_id, &notes, "text/plain").await;

    for name in ["pic-a.png", "pic-b.png"] {
        let file = dir.path().join(name);
        std::fs::write(&file, PNG).unwrap();
        ctx.allow_attachment_path(&file).unwrap();
        send_attachment(&eng, &room_id, &file, "image/png").await;
    }

    send_text(
        &eng,
        &room_id,
        "see https://example.org/a and http://example.com/b.",
    )
    .await;
    send_text(&eng, &room_id, "no links here").await;

    // Sends complete asynchronously on the server; poll until every listing has its entries.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let (media, files, links) = loop {
        let (media, _) = listing(&eng, &room_id, RoomMediaKind::Media, None, 100).await;
        let (files, _) = listing(&eng, &room_id, RoomMediaKind::Files, None, 100).await;
        let (links, _) = listing(&eng, &room_id, RoomMediaKind::Links, None, 100).await;
        if media.len() == 2 && files.len() == 1 && links.len() == 1 {
            break (media, files, links);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "room media never appeared: media={} files={} links={}",
            media.len(),
            files.len(),
            links.len()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    // Media is newest first.
    assert_eq!(media[0].kind, "image");
    assert_eq!(media[1].kind, "image");
    assert!(media[0].body.contains("pic-b"), "{:?}", media[0].body);
    assert!(media[1].body.contains("pic-a"), "{:?}", media[1].body);
    assert!(media[0].timestamp_millis >= media[1].timestamp_millis);
    assert!(
        media[0]
            .uri
            .as_deref()
            .is_some_and(|u| u.starts_with("mxc://")),
        "{:?}",
        media[0].uri
    );

    // Files hold the non-media attachment.
    assert_eq!(files[0].kind, "file");
    assert!(files[0].body.contains("notes.txt"), "{:?}", files[0].body);
    assert!(files[0].uri.is_some());

    // Links hold only messages with a URL, with the URLs extracted and deduplicated.
    assert_eq!(links[0].kind, "link");
    assert_eq!(
        links[0].urls,
        vec!["https://example.org/a", "http://example.com/b"]
    );
    assert!(links[0].uri.is_none());

    // Paging: a one-item page hands back a cursor that yields the next item.
    let (first, next) = listing(&eng, &room_id, RoomMediaKind::Media, None, 1).await;
    assert_eq!(first.len(), 1);
    assert!(first[0].body.contains("pic-b"));
    let next = next.expect("cursor for the remaining history");
    let (second, _) = listing(&eng, &room_id, RoomMediaKind::Media, Some(next), 1).await;
    assert_eq!(second.len(), 1);
    assert!(second[0].body.contains("pic-a"));
    assert_ne!(second[0].event_id, first[0].event_id);
}
