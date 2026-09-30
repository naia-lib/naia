#![allow(
    unused_imports,
    unused_variables,
    unused_must_use,
    unused_mut,
    dead_code,
    for_loops_over_fallibles
)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, Publicity};
use naia_server::{ReplicationConfig, RoomKey, ServerConfig};
use naia_shared::{
    AuthorityError, DisconnectReason, EntityAuthStatus, Protocol, Request, Response, Tick,
};

use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientDisconnectEvent, ClientEntityAuthDeniedEvent,
    ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, ClientRejectEvent,
    ExpectCtx, Position, Scenario, ServerAuthEvent, ServerConnectEvent, ServerDisconnectEvent,
    ToTicks,
};

// Test protocol types (channels and messages)
use naia_test_harness::test_protocol::{
    OrderedChannel, ReliableChannel, RequestResponseChannel, SequencedChannel, TestMessage,
    TestRequest, TestResponse, TickBufferedChannel, UnorderedChannel, UnreliableChannel,
};

mod _helpers;
use _helpers::{
    client_connect, server_and_client_connected, server_and_client_disconnected, test_client_config,
};

// ============================================================================
// DWO-2 (naia reconnect-cancel API) red-first pins
// ============================================================================
// Owner: Nash. Lands on naia dev behind the unreachable DWO feature
// boundary: purely additive client API + a stale-flag reset on the
// disconnect path. No heartbeat/config defaults change, no Cyberlith menu
// semantics in naia.
// ============================================================================

/// Accept a reconnecting client: wait for its fresh auth, accept it, wait
/// for the fresh connect event. Mirrors the accept half of `client_connect`
/// for a client object that already exists.
fn accept_reconnect(scenario: &mut Scenario, client_key: ClientKey, client_auth: &Auth) {
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) =
                server.read_event::<ServerAuthEvent<Auth>>()
            {
                if incoming_client_key == client_key && &incoming_auth == client_auth {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_client_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_client_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.mutate(|_ctx| {});
}

/// Explicit server disconnect, then a fresh authenticated connect on the
/// SAME client object, must settle connected with no second disconnect.
///
/// Red-first: `disconnect_reset_connection` retains `server_disconnect`,
/// so the first `process_all_packets` after the handshake completes takes
/// the disconnect path again — a spurious second disconnect with a wrong
/// `ClientDisconnected` reason, and the client never settles connected.
#[test]
fn explicit_server_disconnect_then_reconnect_stays_connected() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();
    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let auth = Auth::new("client", "password");
    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        auth.clone(),
        test_client_config(),
        test_protocol.clone(),
    );
    scenario.mutate(|_ctx| {});

    // Explicit server-side disconnect (the DWO terminal/transient source).
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.disconnect_user(&client_key));
        });
    });

    // The client observes exactly one disconnect and both sides agree.
    scenario.expect(|ctx| {
        let event = ctx.client(client_key, |c| c.read_event::<ClientDisconnectEvent>());
        (event.is_some() && server_and_client_disconnected(ctx, client_key).is_some()).then_some(())
    });
    scenario.mutate(|_ctx| {});

    // Same-client fresh authenticated reconnect (the grace-expiry path).
    scenario.client_reconnect(client_key);
    accept_reconnect(&mut scenario, client_key, &auth);

    // Settles connected on both sides...
    scenario.expect(|ctx| server_and_client_connected(ctx, client_key));

    // ...and stays there: extra ticks would re-fire a stale
    // `server_disconnect` flag, and any second DisconnectEvent fails this.
    scenario.mutate(|_ctx| {});
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| {
        let no_second = ctx.client(client_key, |c| {
            c.read_event::<ClientDisconnectEvent>().is_none()
        });
        let still_connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        (no_second && still_connected).then_some(())
    });
}

/// Cancelling a pending handshake returns the client to `Disconnected` so a
/// fresh `connect` never panics; cancelling a live connection is a no-op.
///
/// Red-first: `Client::cancel_connect` does not exist yet — this test is
/// compile-red until the API lands. Calling `connect` on the loaded
/// (Connecting) client would panic without the cancel.
#[test]
fn cancel_pending_handshake_then_reconnect_never_panics() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();
    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let auth = Auth::new("client", "password");
    let client_key = scenario.client_start(
        "Client",
        auth.clone(),
        test_client_config(),
        test_protocol.clone(),
    );

    // Handshake in flight (Connecting): cancel must drop the attempt and
    // report Disconnected, never panic, never emit a disconnect event.
    scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client.cancel_connect();
        });
    });
    scenario.expect(|ctx| {
        let status = ctx.client(client_key, |c| c.connection_status());
        let no_event = ctx.client(client_key, |c| {
            c.read_event::<ClientDisconnectEvent>().is_none()
        });
        (status.is_disconnected() && no_event).then_some(())
    });
    scenario.mutate(|_ctx| {});

    // Fresh attempt on the same client connects cleanly.
    scenario.client_reconnect(client_key);
    accept_reconnect(&mut scenario, client_key, &auth);
    scenario.expect(|ctx| server_and_client_connected(ctx, client_key));

    // Cancelling a LIVE connection is a no-op: still connected, no event.
    scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client.cancel_connect();
        });
    });
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| {
        let no_event = ctx.client(client_key, |c| {
            c.read_event::<ClientDisconnectEvent>().is_none()
        });
        let still_connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        (no_event && still_connected).then_some(())
    });
}
