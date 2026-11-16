use std::net::SocketAddr;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::config_bundle::{ChatKey, ConfigState, TagSet};

fn bundle() -> ConfigBundle {
    let mut state = ConfigState {
        settings: json!({"density": "comfortable", "send_on_enter": true}),
        settings_updated_at_ms: 1_000,
        ..ConfigState::default()
    };
    state.tags.insert(
        ChatKey::new("tg:alice", "room-1"),
        TagSet::new(["work".to_string()], 1_100),
    );
    state.export("desk", 2_000)
}

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().expect("literal address")
}

#[test]
fn ticket_roundtrips_through_its_plain_string() {
    let ticket = PairingTicket {
        pairing_id: to_hex(&[7u8; PAIRING_ID_BYTES]),
        endpoint: "192.168.1.20:7000".into(),
        nonce: [9u8; 32],
        lan_hint: Some("alloy.local:7000".into()),
    };
    let text = ticket.to_qr_string();
    assert!(text.starts_with("alloy-pairing-v1|"));
    assert_eq!(PairingTicket::parse(&text).unwrap(), ticket);

    let plain = PairingTicket {
        lan_hint: None,
        ..ticket
    };
    assert_eq!(PairingTicket::parse(&plain.to_qr_string()).unwrap(), plain);

    assert!(matches!(
        PairingTicket::parse("alloy-pairing-v2||||"),
        Err(PairingError::MalformedPayload(_))
    ));
    assert!(matches!(
        PairingTicket::parse("alloy-pairing-v1|zz|127.0.0.1:1|00|"),
        Err(PairingError::MalformedPayload(_))
    ));
}

#[test]
fn codes_use_the_unambiguous_alphabet() {
    for _ in 0..64 {
        let code = random_code();
        assert_eq!(code.len(), 8);
        for ch in code.chars() {
            assert!(
                CODE_ALPHABET.contains(&(ch as u8)),
                "{ch:?} is outside the code alphabet"
            );
            assert!(!"01OILU".contains(ch));
        }
    }
}

#[tokio::test]
async fn loopback_transfers_and_imports_a_bundle() {
    let offer = PairingOffer::bind(bundle(), loopback(), None)
        .await
        .unwrap();
    assert_eq!(offer.code().len(), 8);
    let ticket = offer.ticket().clone();
    let code = offer.code().to_string();
    let offer_task = tokio::spawn(async move {
        let handshake = offer.wait_for_peer().await?;
        let sas = handshake.sas().to_string();
        handshake.finish(true).await?;
        Ok::<String, PairingError>(sas)
    });

    let pending = connect(&ticket.endpoint, &ticket, &code).await.unwrap();
    let client_sas = pending.sas().to_string();
    assert_eq!(client_sas.len(), SAS_DIGITS);
    let received = pending.finish(true).await.unwrap();
    let offer_sas = offer_task.await.unwrap().unwrap();

    assert_eq!(offer_sas, client_sas, "both screens must show the same SAS");
    assert_eq!(received, bundle());

    let mut state = ConfigState::default();
    let report = received.import_into(&mut state).unwrap();
    assert_eq!(report.tags, 1);
    assert!(report.settings);
    assert_eq!(
        state.settings,
        json!({"density": "comfortable", "send_on_enter": true})
    );
}

#[tokio::test]
async fn a_wrong_code_is_rejected_and_no_bundle_crosses() {
    let offer = PairingOffer::bind(bundle(), loopback(), None)
        .await
        .unwrap();
    let ticket = offer.ticket().clone();
    let offer_task = tokio::spawn(async move {
        match offer.wait_for_peer().await {
            Ok(handshake) => {
                handshake.finish(true).await?;
                Ok::<(), PairingError>(())
            }
            Err(err) => Err(err),
        }
    });

    let pending = connect(&ticket.endpoint, &ticket, "ZZZZZZZZ")
        .await
        .unwrap();
    assert!(pending.finish(true).await.is_err());
    let err = offer_task.await.unwrap().unwrap_err();
    assert!(
        matches!(err, PairingError::CodeMismatch),
        "expected CodeMismatch, got {err:?}"
    );
}

#[tokio::test]
async fn an_expired_code_never_listens() {
    let offer = PairingOffer::bind_with_ttl(bundle(), loopback(), None, Duration::ZERO)
        .await
        .unwrap();
    let err = match offer.wait_for_peer().await {
        Ok(_) => panic!("an expired offer accepted a peer"),
        Err(err) => err,
    };
    assert!(matches!(err, PairingError::CodeExpired(_)));
}

/// A machine in the middle runs two independent handshakes: one with the real offer, one with the
/// real client. It knows the code (the user typed it on the client, and the relay forwards it), so
/// only the SAS can expose it. The two sides therefore derive different digits, and the client
/// refuses before any bundle byte is sent.
#[tokio::test]
async fn mitm_proxy_yields_different_sas_on_each_side_and_both_refuse() {
    let real_offer = PairingOffer::bind(bundle(), loopback(), None)
        .await
        .unwrap();
    let real_ticket = real_offer.ticket().clone();
    let real_code = real_offer.code().to_string();
    let real_offer_task = tokio::spawn(async move {
        let handshake = real_offer.wait_for_peer().await?;
        let sas = handshake.sas().to_string();
        let outcome = handshake.finish(false).await;
        Ok::<(String, Result<(), PairingError>), PairingError>((sas, outcome))
    });

    // The proxy's own offer toward the client.
    let proxy_offer = PairingOffer::bind(bundle(), loopback(), None)
        .await
        .unwrap();
    let proxy_ticket = proxy_offer.ticket().clone();
    let proxy_code = proxy_offer.code().to_string();
    let proxy_offer_task = tokio::spawn(async move {
        let handshake = proxy_offer.wait_for_peer().await?;
        let sas = handshake.sas().to_string();
        let outcome = handshake.finish(false).await;
        Ok::<(String, Result<(), PairingError>), PairingError>((sas, outcome))
    });

    // The proxy acting as a client against the real offer.
    let proxy_client_task = tokio::spawn(async move {
        let pending = connect(&real_ticket.endpoint, &real_ticket, &real_code).await?;
        let sas = pending.sas().to_string();
        let outcome = pending.finish(false).await;
        Ok::<(String, Result<ConfigBundle, PairingError>), PairingError>((sas, outcome))
    });

    // The real client, which believes it is talking to the real offer.
    let client = connect(&proxy_ticket.endpoint, &proxy_ticket, &proxy_code)
        .await
        .unwrap();
    let client_sas = client.sas().to_string();
    let outcome = client.finish(false).await;

    let (real_sas, real_outcome) = real_offer_task.await.unwrap().unwrap();
    let (proxy_offer_sas, proxy_offer_outcome) = proxy_offer_task.await.unwrap().unwrap();
    let (proxy_client_sas, proxy_client_outcome) = proxy_client_task.await.unwrap().unwrap();

    assert_eq!(real_sas, proxy_client_sas, "handshake with the real offer");
    assert_eq!(client_sas, proxy_offer_sas, "handshake with the client");
    assert_ne!(
        client_sas, real_sas,
        "the proxy must not reproduce the real SAS on both screens"
    );

    assert!(matches!(real_outcome, Err(PairingError::Declined(_))));
    assert!(matches!(
        proxy_offer_outcome,
        Err(PairingError::Declined(_))
    ));
    assert!(matches!(
        proxy_client_outcome,
        Err(PairingError::Declined(_))
    ));
    assert!(matches!(outcome, Err(PairingError::Declined(_))));
}

#[tokio::test]
async fn a_ticket_with_a_different_pairing_id_is_refused() {
    let offer = PairingOffer::bind(bundle(), loopback(), None)
        .await
        .unwrap();
    let mut ticket = offer.ticket().clone();
    let code = offer.code().to_string();
    let offer_task = tokio::spawn(async move { offer.wait_for_peer().await.map(|_| ()) });
    ticket.pairing_id = to_hex(&[0u8; PAIRING_ID_BYTES]);
    let err = match connect(&ticket.endpoint, &ticket, &code).await {
        Ok(_) => panic!("connect accepted a ticket with a foreign pairing id"),
        Err(err) => err,
    };
    assert!(matches!(err, PairingError::Protocol(_)));
    assert!(
        offer_task.await.unwrap().is_err(),
        "the offer must not complete a handshake the client refused"
    );
}
