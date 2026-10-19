#![recursion_limit = "256"]

// Integration test against a running Synapse server
// Run with: ALLOY_TEST_HS=http://localhost:8008 cargo test -p alloy-core --test synapse_integration -- --nocapture

use alloy_core::{AlloyClient, AlloyError, InMemorySecretStore};
use std::time::Duration;
use tempfile::TempDir;

async fn create_test_client(hs: &str, tmpdir: &TempDir) -> Result<AlloyClient, AlloyError> {
    AlloyClient::new(
        hs,
        tmpdir.path(),
        Some(Box::new(InMemorySecretStore::new())),
    )
    .await
}

#[tokio::test]
async fn test_alice_login_and_sync() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    let tmpdir = TempDir::new().unwrap();
    let alice = create_test_client(&hs, &tmpdir).await.unwrap();

    // Login
    alice
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .expect("Alice login failed");

    assert!(alice.user_id().is_some());
    println!("✓ Alice logged in as {}", alice.user_id().unwrap());

    // Sync once
    alice.sync_once().await.expect("First sync failed");
    println!("✓ Alice synced once");
}

#[tokio::test]
async fn test_alice_create_room() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    let tmpdir = TempDir::new().unwrap();
    let alice = create_test_client(&hs, &tmpdir).await.unwrap();

    alice
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .unwrap();
    alice.sync_once().await.unwrap();

    // Create room
    let room_id = alice
        .create_room("Integration Test Room", false)
        .await
        .expect("Room creation failed");

    println!("✓ Alice created room: {}", room_id.0);

    // Verify room exists
    let inner_client = alice.inner_client();
    let parsed_id = matrix_sdk::ruma::RoomId::parse(&room_id.0).unwrap();
    let room = inner_client.get_room(&parsed_id);
    assert!(room.is_some());
    println!("✓ Room exists in client");
}

#[tokio::test]
async fn test_alice_send_message_with_local_echo() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    let tmpdir = TempDir::new().unwrap();
    let alice = create_test_client(&hs, &tmpdir).await.unwrap();

    alice
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .unwrap();
    alice.sync_once().await.unwrap();

    // Create room
    let room_id = alice.create_room("Test Room", false).await.unwrap();
    alice.sync_once().await.unwrap();
    println!("✓ Room created: {}", room_id.0);
    // Get timeline FIRST before sending so local echo is captured
    let timeline = alice
        .timeline(&room_id)
        .await
        .expect("Timeline creation failed");

    let (items_before, mut stream) = timeline.subscribe().await;
    println!("✓ Timeline initial items: {}", items_before.len());

    // Send message through timeline to generate local echo
    use futures::StreamExt;
    use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
    let _handle = timeline
        .send(RoomMessageEventContent::text_plain("Hello from Alice!").into())
        .await
        .expect("Timeline send failed");

    println!("✓ Alice queued message via timeline");

    // The stream should yield the local echo
    use matrix_sdk_ui::eyeball_im::VectorDiff;
    let diffs = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout waiting for timeline diff")
        .expect("Stream closed unexpectedly");

    println!("✓ Received {} diffs from timeline stream", diffs.len());
    let mut found_local_echo = false;
    for diff in &diffs {
        if let VectorDiff::PushBack { value } = diff
            && let Some(event_item) = value.as_event()
            && event_item.is_local_echo()
        {
            println!(
                "✓ Event local echo status: is_local_echo={}",
                event_item.is_local_echo()
            );
            found_local_echo = true;
        }
    }
    assert!(
        found_local_echo,
        "Should have received local echo in diff stream"
    );
}
#[tokio::test]
async fn test_bob_joins_and_sees_message() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    // Alice creates room and sends message
    let tmpdir_alice = TempDir::new().unwrap();
    let alice = create_test_client(&hs, &tmpdir_alice).await.unwrap();

    alice
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .unwrap();
    alice.sync_once().await.unwrap();

    let room_id = alice.create_room("Test Room", false).await.unwrap();
    alice.sync_once().await.unwrap();
    println!("✓ Alice created room: {}", room_id.0);

    // Alice invites Bob
    alice
        .invite_user(&room_id, "@bob:localhost")
        .await
        .expect("Alice invite Bob failed");
    println!("✓ Alice invited Bob");

    // Bob joins the room
    let tmpdir_bob = TempDir::new().unwrap();
    let bob = create_test_client(&hs, &tmpdir_bob).await.unwrap();

    bob.login_password("bob", "testpassword123!", Some("bob-device"))
        .await
        .unwrap();
    bob.sync_once().await.unwrap();
    println!("✓ Bob logged in");

    bob.join_room(&room_id).await.expect("Bob join failed");
    println!("✓ Bob joined room");
    bob.sync_once().await.unwrap();

    // Bob subscribes to timeline BEFORE Alice sends
    let timeline = bob.timeline(&room_id).await.expect("Timeline failed");
    let (_items, mut stream) = timeline.subscribe().await;

    // Alice sends message now that Bob is subscribed
    let event_id = alice
        .send_message(&room_id, "Hello from Alice to Bob!")
        .await
        .unwrap();
    println!("✓ Alice sent message: {}", event_id.0);

    // Bob syncs to fetch the new event
    bob.sync_once().await.unwrap();
    println!("✓ Bob synced");

    // Bob receives the message over timeline stream!
    use futures::StreamExt;
    let updates = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout waiting for Bob's timeline update")
        .expect("Stream closed");

    println!(
        "✓ Bob received {} timeline updates via stream!",
        updates.len()
    );
    assert!(
        !updates.is_empty(),
        "Bob should receive timeline updates with Alice's message"
    );
}

#[tokio::test]
async fn test_session_restore() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    let tmpdir = TempDir::new().unwrap();
    let secret_store = Box::new(InMemorySecretStore::new());

    // First client: login
    let client1 = AlloyClient::new(&hs, tmpdir.path(), Some(secret_store.clone()))
        .await
        .unwrap();

    client1
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .unwrap();
    client1.sync_once().await.unwrap();

    let user_id_1 = client1.user_id().unwrap().to_string();
    println!("✓ Client 1 logged in as {}", user_id_1);

    // Second client: same store path and secret store
    let client2 = AlloyClient::new(&hs, tmpdir.path(), Some(secret_store.clone()))
        .await
        .unwrap();

    // Client 2 should restore session from the SQLite store
    let session_restored = client2.restore_session().await.unwrap();
    assert!(
        session_restored,
        "Session should be restored from SQLite store"
    );

    let user_id_2 = client2.user_id().unwrap().to_string();
    assert_eq!(user_id_1, user_id_2);
    println!("✓ Client 2 restored session as {}", user_id_2);

    // Client 2 should be able to sync without re-login
    client2.sync_once().await.unwrap();
    println!("✓ Client 2 synced without re-login");
}

#[tokio::test]
async fn test_room_list_service() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    let tmpdir = TempDir::new().unwrap();
    let alice = create_test_client(&hs, &tmpdir).await.unwrap();

    alice
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .unwrap();
    alice.sync_once().await.unwrap();
    // Get room list stream BEFORE creating room
    use futures::StreamExt;
    let (_initial_rooms, mut stream) = alice.room_list_stream().await;
    println!("✓ Alice subscribed to room_list_stream");

    // Alice creates a new room
    let expected_name = "Room List Test Room";
    let room_id = alice.create_room(expected_name, false).await.unwrap();
    println!("✓ Alice created room: {}", room_id.0);

    // Alice syncs to receive the room event into local state
    alice.sync_once().await.unwrap();
    println!("✓ Alice synced after room creation");

    // Receive diffs from room_list_stream
    let diffs = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout waiting for room list diff")
        .expect("Room list stream closed");

    println!("✓ Alice received {} room list diffs", diffs.len());

    // Find the created room in the diff stream and assert name and ID
    let mut found_room = false;
    for diff in diffs {
        let summaries: Vec<alloy_protocol::RoomSummary> = match diff {
            alloy_protocol::VectorDiffDto::PushBack { value }
            | alloy_protocol::VectorDiffDto::PushFront { value }
            | alloy_protocol::VectorDiffDto::Insert { value, .. }
            | alloy_protocol::VectorDiffDto::Set { value, .. } => vec![value],
            alloy_protocol::VectorDiffDto::Append { values }
            | alloy_protocol::VectorDiffDto::Reset { values } => values,
            _ => vec![],
        };

        for summary in summaries {
            if summary.id.0 == room_id.0 {
                println!("✓ Verified room ID matches: {}", summary.id.0);
                println!("✓ Room name in summary: {:?}", summary.name);
                assert_eq!(summary.id.0, room_id.0);
                assert_eq!(summary.name, Some(expected_name.to_string()));
                found_room = true;
            }
        }
    }

    assert!(
        found_room,
        "Stream must yield VectorDiffDto<RoomSummary> with created room"
    );

    // Verify RoomListService works as well
    let room_list_svc = alice.room_list_service().await.unwrap();
    let _all_rooms = room_list_svc.all_rooms().await.unwrap();
    println!("✓ RoomListService verified (MSC4186 simplified sliding sync)");
}

#[tokio::test]
async fn test_e2ee_room_creation() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    // Alice creates encrypted room
    let tmpdir_alice = TempDir::new().unwrap();
    let alice = create_test_client(&hs, &tmpdir_alice).await.unwrap();
    alice
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .unwrap();
    alice.sync_once().await.unwrap();
    println!("✓ Alice logged in");

    let room_id = alice
        .create_room("Encrypted Test Room", true)
        .await
        .expect("Encrypted room creation failed");
    alice.sync_once().await.unwrap();
    println!("✓ Alice created encrypted room: {}", room_id.0);

    // Alice invites Bob
    alice
        .invite_user(&room_id, "@bob:localhost")
        .await
        .expect("Alice invite Bob failed");
    println!("✓ Alice invited Bob");

    // Bob logs in
    let tmpdir_bob = TempDir::new().unwrap();
    let bob = create_test_client(&hs, &tmpdir_bob).await.unwrap();
    bob.login_password("bob", "testpassword123!", Some("bob-device"))
        .await
        .unwrap();
    bob.sync_once().await.unwrap();
    println!("✓ Bob logged in");

    // Bob joins room
    bob.join_room(&room_id).await.expect("Bob join failed");
    bob.sync_once().await.unwrap();
    println!("✓ Bob joined encrypted room and synced");

    // Alice syncs to discover Bob and share encryption key
    alice.sync_once().await.unwrap();
    println!("✓ Alice synced (key sharing negotiated)");

    // Bob subscribes to timeline BEFORE Alice sends
    let timeline = bob.timeline(&room_id).await.expect("Bob timeline failed");
    let (_items, mut stream) = timeline.subscribe().await;
    println!("✓ Bob subscribed to timeline");

    // Alice sends encrypted message
    let plaintext = "Secret message from Alice to Bob!";
    alice
        .send_message(&room_id, plaintext)
        .await
        .expect("Alice send encrypted message failed");
    println!("✓ Alice sent encrypted message");

    // Bob syncs to receive the Megolm session key and encrypted event
    bob.sync_once().await.unwrap();
    println!("✓ Bob synced to receive event");

    // Bob receives timeline update with decrypted message
    use futures::StreamExt;
    let updates = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout waiting for Bob's timeline update")
        .expect("Stream closed");

    println!("✓ Bob received {} timeline updates", updates.len());

    // Verify the message is decrypted (not UTD)
    use matrix_sdk_ui::eyeball_im::VectorDiff;
    let mut found_plaintext = false;
    for diff in updates {
        if let VectorDiff::PushBack { value } = diff
            && let Some(event_item) = value.as_event()
            && let Some(msg) = event_item.content().as_message()
        {
            println!("✓ Event body: '{}'", msg.body());
            if msg.body() == plaintext {
                found_plaintext = true;
            }
        }
    }

    assert!(
        found_plaintext,
        "Bob should receive the plaintext message, not UTD"
    );
    println!("✓ E2EE message decrypted successfully!");
}
