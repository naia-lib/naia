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

/// Cancelling while the server holds a pending, unaccepted auth, then
/// reconnecting on the same client, must settle connected with no spurious
/// client events.
///
/// Red-first: the cancelled attempt's auth already reached the server, so
/// its pending state stays live there. The reconnect's fresh auth must
/// surface as a second servable AuthEvent — the stale first attempt must
/// neither shadow it (reconnect stalls pre-connect) nor later complete
/// into a phantom session (spurious second disconnect kills the new one).
#[test]
fn cancel_during_auth_phase_then_reconnect_settles_connected() {
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

    // Auth phase: the server holds a pending, unaccepted auth for this key.
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) =
                server.read_event::<ServerAuthEvent<Auth>>()
            {
                if incoming_client_key == client_key && incoming_auth == auth {
                    return Some(());
                }
            }
            None
        })
    });

    // Cancel mid-auth: back to Disconnected, nothing emitted.
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

    // Fresh attempt on the same client: the server must serve the second
    // auth and the session must settle connected on both sides.
    scenario.client_reconnect(client_key);
    accept_reconnect(&mut scenario, client_key, &auth);
    scenario.expect(|ctx| server_and_client_connected(ctx, client_key));

    // ...and stay there: extra ticks, no second disconnect, still connected.
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

/// Client-initiated disconnect, then a fresh authenticated connect on the
/// SAME client object, must settle connected with exactly one disconnect
/// event carrying the `ClientDisconnected` reason.
///
/// Red-first: `disconnect()` latches `manual_disconnect`, the last
/// retained-flag source without a same-client reconnect pin
/// (`server_disconnect` and the timeout latch both have one). If the
/// reset ever stops clearing it, the first post-handshake tick re-takes
/// the disconnect path with a second event and the client never settles.
#[test]
fn manual_disconnect_then_reconnect_stays_connected() {
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

    // Orderly client-initiated teardown.
    scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client.disconnect();
        });
    });

    // Exactly one disconnect event with the self-hangup reason, and both
    // sides agree the session is gone.
    let mut reason_seen = None;
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            if let Some((reason, _message)) = client.read_event::<ClientDisconnectEvent>() {
                reason_seen = Some(reason);
            }
        });
        (reason_seen.is_some() && server_and_client_disconnected(ctx, client_key).is_some())
            .then_some(())
    });
    assert_eq!(
        reason_seen,
        Some(DisconnectReason::ClientDisconnected),
        "an orderly client teardown must report ClientDisconnected exactly once"
    );
    scenario.mutate(|_ctx| {});

    // Same-client fresh authenticated reconnect settles connected...
    scenario.client_reconnect(client_key);
    accept_reconnect(&mut scenario, client_key, &auth);
    scenario.expect(|ctx| server_and_client_connected(ctx, client_key));

    // ...and stays there: extra ticks, no second disconnect, still connected.
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

/// Timeout-driven (not explicit) disconnect, then a fresh authenticated
/// connect on the SAME client object, must settle connected with no second
/// disconnect.
///
/// Red-first: the timeout source funnels through the same
/// `disconnect_reset_connection` as the explicit path, so DWO-2's
/// stale-flag reset should already cover it — this pins that. The reason
/// assertion is the tripwire: a retained `server_disconnect` flag would
/// re-take the disconnect path with a wrong `ClientDisconnected` reason
/// instead of settling, and any `should_drop` latch surviving the reset
/// would do the same with a second `TimedOut`.
#[test]
fn timeout_disconnect_then_reconnect_stays_connected() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let mut server_config = ServerConfig::default();
    server_config.connection.heartbeat_interval = Duration::from_millis(100);
    server_config.connection.disconnection_timeout_duration = Duration::from_millis(200);
    scenario.server_start(server_config, test_protocol.clone());

    let mut client_config = test_client_config();
    client_config.connection.heartbeat_interval = Duration::from_millis(100);
    client_config.connection.disconnection_timeout_duration = Duration::from_millis(200);

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let auth = Auth::new("client", "password");
    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        auth.clone(),
        client_config,
        test_protocol.clone(),
    );
    scenario.mutate(|_ctx| {});

    // Silence the link: both sides must time out on their own — no
    // explicit disconnect from either end.
    scenario.pause_traffic();

    let mut client_timed_out = false;
    let mut server_timed_out = false;
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            if let Some((reason, _message)) = client.read_event::<ClientDisconnectEvent>() {
                if reason == DisconnectReason::TimedOut {
                    client_timed_out = true;
                }
            }
        });
        ctx.server(|server| {
            while let Some(disconnected_key) = server.read_event::<ServerDisconnectEvent>() {
                if disconnected_key == client_key {
                    server_timed_out = true;
                }
            }
        });
        (client_timed_out && server_timed_out).then_some(())
    });
    scenario.mutate(|_ctx| {});

    // Link back, same-client fresh authenticated reconnect (grace expiry).
    scenario.resume_traffic();
    scenario.client_reconnect(client_key);
    accept_reconnect(&mut scenario, client_key, &auth);

    // Settles connected on both sides...
    scenario.expect(|ctx| server_and_client_connected(ctx, client_key));

    // ...and stays there: extra ticks, no second disconnect, still connected.
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
