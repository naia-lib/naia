//! Typed refusals from `send_request` (N1-b).
//!
//! Every `send_request` path must return the typed variants the error enums
//! already own (`UserNotFound` / `MessageQueueFull`) instead of untyped
//! `Message(String)` refusals, mirroring `send_message`. Slag collapsed
//! every naia send error into a unit error meaning *disconnected*; a healthy
//! connected user with a momentarily full queue permanently lost an asset.

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, NaiaClientError};
use naia_server::{NaiaServerError, ServerConfig, ServerMode};
use naia_test_harness::test_protocol::{RequestResponseChannel, TestRequest};
use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientKey, Scenario, ServerAuthEvent, ServerConnectEvent,
};

fn test_client_config() -> ClientConfig {
    ClientConfig {
        send_handshake_interval: Duration::from_millis(0),
        jitter_buffer: JitterBufferType::Bypass,
        ..Default::default()
    }
}

/// Bring up server, connect one client. Returns the connected client_key.
fn server_with_one_client(scenario: &mut Scenario) -> ClientKey {
    let test_protocol = protocol();
    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let client_auth = Auth::new("alice", "secret");
    let client_key = scenario.client_start(
        "alice",
        client_auth.clone(),
        test_client_config(),
        test_protocol,
    );

    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _)) = server.read_event::<ServerAuthEvent<Auth>>() {
                (incoming_key == client_key).then_some(())
            } else {
                None
            }
        })
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    scenario.expect(|ctx| {
        ctx.server(|server| {
            if server.read_event::<ServerConnectEvent>().is_some() {
                Some(())
            } else {
                None
            }
        })
    });

    scenario.expect(|ctx| {
        let connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_key));
        let _ = ctx.client(client_key, |c| c.read_event::<ClientConnectEvent>());
        (connected && user_exists).then_some(())
    });

    client_key
}

#[test]
fn client_send_request_reports_message_queue_full_when_channel_queue_fills() {
    let mut scenario = Scenario::new(ServerMode::Resident);
    let client_key = server_with_one_client(&mut scenario);
    let request = TestRequest::new("q");

    // Fill the reliable channel queue without stepping any ticks, so no
    // ACK can drain it mid-fill. Depth-agnostic: loop to the refusal.
    let (accepted, terminal) = scenario.mutate(|ctx| {
        ctx.client(client_key, |c| {
            let mut accepted = 0u32;
            loop {
                match c.send_request::<RequestResponseChannel, _>(&request) {
                    Ok(_) => {
                        accepted += 1;
                        assert!(
                            accepted <= 2048,
                            "queue never refused after 2048 sends; cap is missing"
                        );
                    }
                    Err(e) => break (accepted, e),
                }
            }
        })
    });

    assert!(
        accepted >= 1,
        "at least one request must be accepted before the refusal"
    );
    assert!(
        matches!(terminal, NaiaClientError::MessageQueueFull),
        "expected typed MessageQueueFull, got {terminal:?}"
    );
}

#[test]
fn harness_send_request_to_unknown_client_returns_user_not_found() {
    let mut scenario = Scenario::new(ServerMode::Resident);
    let _client_key = server_with_one_client(&mut scenario);
    let request = TestRequest::new("q");

    let err = scenario.mutate(|ctx| {
        ctx.server(|s| {
            s.send_request::<RequestResponseChannel, _>(&ClientKey::invalid(), &request)
                .unwrap_err()
        })
    });

    assert!(
        matches!(err, NaiaServerError::UserNotFound),
        "expected typed UserNotFound, got {err:?}"
    );
}
