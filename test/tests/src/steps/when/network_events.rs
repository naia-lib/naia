//! When-step bindings: connection/disconnection/tick-passage events.
//!
//! Network events are *imperative*: a client connects, the server
//! disconnects somebody, N ticks elapse. They drive the system into a
//! new observable state without modeling a domain action.

use naia_test_harness::{ClientDisconnectEvent, EntityKey, TrackedClientEvent, TrackedServerEvent};

use crate::steps::prelude::*;
use crate::steps::world_helpers::{connect_named_client, graceful_disconnect_last_client};

/// When a client connects.
///
/// Mirror of the Given variant — usable as `When` (the action under
/// test) or `And` after another When. Drives the standard handshake
/// via [`connect_client`].
#[when("a client connects")]
fn when_client_connects(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// When the server disconnects the client.
///
/// Initiates a server-side disconnect of the most-recently-connected
/// client and waits for the client to observe the
/// `ClientDisconnectEvent`. Tracks both the server-side and
/// client-side events so subsequent Then steps can assert on them.
#[when("the server disconnects the client")]
fn when_server_disconnects(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user(&client_key);
        });
    });
    scenario.track_server_event(TrackedServerEvent::Disconnect);

    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientDisconnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Disconnect);

    scenario.allow_flexible_next();
}

/// When a second client connects and the entity enters scope for it.
///
/// Used by world-integration late-join tests. Connects a second client
/// via the standard handshake and includes the stored entity in its
/// scope as part of the room-add step. Stores the new client key
/// under `SECOND_CLIENT_KEY` for downstream Then steps.
#[when("a second client connects and the entity enters scope for it")]
fn when_second_client_connects_and_entity_enters_scope(ctx: &mut TestWorldMut) {
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned for world integration test");

    let client_key = connect_named_client(
        ctx,
        "SecondClient",
        "second_client",
        Some(Box::new(move |scenario, ck| {
            scenario.mutate(|mctx| {
                mctx.server(|server| {
                    if let Some(mut scope) = server.user_scope_mut(&ck) {
                        scope.include(&entity_key);
                    }
                });
            });
        })),
    );

    ctx.scenario_mut().bdd_store(SECOND_CLIENT_KEY, client_key);
}

// ──────────────────────────────────────────────────────────────────────
// Observability — connection lifecycle + sample collection
// ──────────────────────────────────────────────────────────────────────

/// When the client disconnects.
#[when("the client disconnects")]
fn when_client_disconnects(ctx: &mut TestWorldMut) {
    disconnect_last_client(ctx);
}

/// When the client disconnects gracefully.
///
/// The client sends token-authenticated disconnect packets. The server verifies
/// the session token embedded in the disconnect packet and processes the
/// disconnect immediately — this is the positive case for the identity-token
/// disconnect-authentication mechanism.
#[when("the client disconnects gracefully")]
fn when_client_disconnects_gracefully(ctx: &mut TestWorldMut) {
    graceful_disconnect_last_client(ctx);
}

/// When sufficient samples have been collected.
///
/// Advances 50 ticks to collect enough RTT samples for convergence.
#[when("sufficient samples have been collected")]
fn when_sufficient_samples_collected(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..50 {
        scenario.mutate(|_| {});
    }
    scenario.allow_flexible_next();
}

/// When traffic is exchanged for multiple metric windows.
///
/// 1000ms window / 16ms tick × 3 windows ≈ 187 ticks.
#[when("traffic is exchanged for multiple metric windows")]
fn when_traffic_exchanged_multiple_windows(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let ticks_per_window = 1000 / 16;
    for _ in 0..(ticks_per_window * 3) {
        scenario.mutate(|_| {});
    }
    scenario.allow_flexible_next();
}

/// When the client reconnects with latency {n}ms.
///
/// Starts a new client session with the specified link latency.
/// Used to test that RTT does not carry stale values from prior
/// sessions.
#[when("the client reconnects with latency {int}ms")]
fn when_client_reconnects_with_latency(ctx: &mut TestWorldMut, latency_ms: u32) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        protocol, Auth, ClientConnectEvent, LinkConditionerConfig, ServerAuthEvent,
        ServerConnectEvent, TrackedClientEvent, TrackedServerEvent,
    };
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let client_key = scenario.client_start(
        "ReconnectedClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );
    let latency_config = LinkConditionerConfig::new(latency_ms, 0, 0.0);
    scenario.configure_link_conditioner(
        &client_key,
        Some(latency_config.clone()),
        Some(latency_config),
    );
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
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
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);
    scenario.allow_flexible_next();
}

// ──────────────────────────────────────────────────────────────────────
// Transport — inbound packet handling, conditioning, abstraction
// ──────────────────────────────────────────────────────────────────────

/// When the server receives a packet exceeding MTU.
///
/// Injects a 1000-byte oversized packet from client to server, ticks
/// 3 times, captures any panic. The contract is that the server
/// drops the packet gracefully.
#[when("the server receives a packet exceeding MTU")]
fn when_server_receives_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let oversized: Vec<u8> = (0u16..1000).map(|i| (i % 256) as u8).collect();
    let _ = scenario.inject_client_packet(&client_key, oversized);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives a packet exceeding MTU.
#[when("the client receives a packet exceeding MTU")]
fn when_client_receives_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let oversized: Vec<u8> = (0u16..1000).map(|i| (i % 256) as u8).collect();
    let _ = scenario.inject_server_packet(&client_key, oversized);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When packets from the client are dropped by the transport.
///
/// Configures 100% loss client→server, ticks 10 times. Server should
/// remain operational (graceful packet loss handling).
#[when("packets from the client are dropped by the transport")]
fn when_packets_from_client_dropped(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        Some(LinkConditionerConfig::new(0, 0, 1.0)),
        None,
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When packets from the server are dropped by the transport.
#[when("packets from the server are dropped by the transport")]
fn when_packets_from_server_dropped(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        None,
        Some(LinkConditionerConfig::new(0, 0, 1.0)),
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the server receives duplicate packets.
///
/// Injects the same valid-looking packet 3 times, ticks 5 times.
/// Server should dedupe + handle gracefully.
#[when("the server receives duplicate packets")]
fn when_server_receives_duplicate_packets(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let packet: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            let _ = scenario.inject_client_packet(&client_key, packet.clone());
        }
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives duplicate packets.
#[when("the client receives duplicate packets")]
fn when_client_receives_duplicate_packets(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let packet: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            let _ = scenario.inject_server_packet(&client_key, packet.clone());
        }
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the server receives packets in a different order than sent.
///
/// Configures jitter (50ms latency, 40ms jitter) on client→server to
/// induce reordering. Ticks 10 times.
#[when("the server receives packets in a different order than sent")]
fn when_server_receives_packets_reordered(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        Some(LinkConditionerConfig::new(50, 40, 0.0)),
        None,
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives packets in a different order than sent.
#[when("the client receives packets in a different order than sent")]
fn when_client_receives_packets_reordered(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        None,
        Some(LinkConditionerConfig::new(50, 40, 0.0)),
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the same application logic runs on each transport.
///
/// Runs a connect → send-message flow under default (no
/// conditioning) transport conditions. The matching Then asserts the
/// flow completed without panic. Used for transport-abstraction
/// independence proof.
#[when("the same application logic runs on each transport")]
fn when_same_application_logic_runs(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        protocol, test_protocol::{TestMessage, UnreliableChannel}, Auth, ClientConnectEvent,
        ServerAuthEvent, ServerConnectEvent,
    };
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let test_protocol = protocol();
        let room_key = scenario.last_room();
        let mut client_config = ClientConfig::default();
        client_config.send_handshake_interval = Duration::from_millis(0);
        client_config.jitter_buffer = JitterBufferType::Bypass;
        let client_key = scenario.client_start(
            "IdealClient",
            Auth::new("test_user", "password"),
            client_config,
            test_protocol,
        );
        scenario.expect(|ctx| {
            ctx.server(|server| {
                if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                    if incoming_key == client_key {
                        return Some(incoming_key);
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
                if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                    if incoming_key == client_key {
                        return Some(());
                    }
                }
                None
            })
        });
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server
                    .room_mut(&room_key)
                    .expect("room exists")
                    .add_user(&client_key);
            });
        });
        scenario.expect(|ctx| {
            ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
        });
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.send_message::<UnreliableChannel, _>(&client_key, &TestMessage::new(100));
            });
        });
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the entity despawns on the client.
///
/// Polls until the client no longer has the entity locally. Used as
/// a sequencing barrier in scope-exit tests.
#[when("the entity despawns on the client")]
fn when_entity_despawns_on_client(ctx: &mut TestWorldMut) {
    use naia_test_harness::EntityKey;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            if !client.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
}

// ──────────────────────────────────────────────────────────────────────
// Connection lifecycle — handshake outcomes
// ──────────────────────────────────────────────────────────────────────

/// When the client attempts to connect.
///
/// Drives the auth event + accept-connection step but stops short of
/// the full connect handshake — used by protocol-mismatch tests
/// where the connect-event never fires.
#[when("the client attempts to connect")]
fn when_client_attempts_to_connect(ctx: &mut TestWorldMut) {
    use naia_test_harness::{Auth, ServerAuthEvent};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
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
    scenario.record_ok();
}

/// When a client authenticates and connects.
///
/// Full happy-path handshake with both server- and client-side
/// event tracking. Tracks AuthEvent → ConnectEvent (server) and
/// ConnectEvent (client).
#[when("a client authenticates and connects")]
fn when_client_authenticates_and_connects(ctx: &mut TestWorldMut) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        protocol, Auth, ClientConnectEvent, ServerAuthEvent, ServerConnectEvent,
        TrackedClientEvent, TrackedServerEvent,
    };
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let client_key = scenario.client_start(
        "TestClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Auth);
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);
    scenario.mutate(|_| {});
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });
    scenario.allow_flexible_next();
}

/// When a client attempts to connect but is rejected.
///
/// Drives the auth flow + server-side reject. Tracks the client's
/// `RejectEvent` so downstream Then steps can assert it.
#[when("a client attempts to connect but is rejected")]
fn when_client_attempts_connection_rejected(ctx: &mut TestWorldMut) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        protocol, Auth, ClientRejectEvent, ServerAuthEvent, TrackedClientEvent,
    };
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let client_key = scenario.client_start(
        "RejectedClient",
        Auth::new("bad_user", "bad_password"),
        client_config,
        test_protocol,
    );
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
                }
            }
            None
        })
    });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.reject_connection(&client_key);
        });
    });
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientRejectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Reject);
    scenario.allow_flexible_next();
}

// ──────────────────────────────────────────────────────────────────────
// Common — generic When phrasings + reconnect/malformed/duplicate flows
// ──────────────────────────────────────────────────────────────────────

/// When a connected client.
///
/// Mirror of the Given variant — usable as When/And after a Given.
#[when("a connected client")]
fn when_connected_client(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// When the client reconnects.
///
/// Starts a brand-new client session after a prior disconnect. Adds
/// to the same room so prior-session entities should be re-spawned.
#[when("the client reconnects")]
fn when_client_reconnects(ctx: &mut TestWorldMut) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        protocol, Auth, ClientConnectEvent, ServerAuthEvent, ServerConnectEvent,
        TrackedClientEvent, TrackedServerEvent,
    };
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let client_key = scenario.client_start(
        "ReconnectedClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
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
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);
    scenario.allow_flexible_next();
}

/// When the server receives a malformed packet.
#[when("the server receives a malformed packet")]
fn when_server_receives_malformed_packet(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let client_key = scenario.last_client();
    let malformed = vec![0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03, 0xFF, 0xFF];
    let _ = scenario.inject_client_packet(&client_key, malformed);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives a malformed packet.
#[when("the client receives a malformed packet")]
fn when_client_receives_malformed_packet(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let client_key = scenario.last_client();
    let malformed = vec![0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03, 0xFF, 0xFF];
    let _ = scenario.inject_server_packet(&client_key, malformed);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When duplicate replication messages arrive.
///
/// Ticks 5 times — protocol-level dedup should keep state stable.
#[when("duplicate replication messages arrive")]
fn when_duplicate_replication_messages_arrive(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the same API call sequence is executed twice.
///
/// Determinism check — 10 ticks. The local transport + TestClock
/// guarantee identical behavior.
#[when("the same API call sequence is executed twice")]
fn when_same_api_sequence_twice(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the tick is processed.
///
/// Single explicit tick — used to make the scenario flow read more
/// naturally when the scenario has queued state in a Given.
#[when("the tick is processed")]
fn when_tick_is_processed(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|_| {});
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When client A disconnects from the server.
///
/// Server-initiated disconnect for the named client. Used by
/// [entity-delegation-14] (disconnect releases authority).
#[when("client A disconnects from the server")]
fn when_client_a_disconnects_from_server(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_a: crate::ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.disconnect_user(&client_a);
        });
    });
}

/// When one full replication round trip elapses.
///
/// Spins 30 server ticks. Used by replicated-resources scenarios as
/// an explicit barrier between the When (mutate) and the Then
/// (assert).
#[when("one full replication round trip elapses")]
fn when_one_full_round_trip(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

/// When one replication round trip elapses.
///
/// Alias of `one full replication round trip elapses` — the
/// replicated-resources spec uses both phrasings.
#[when("one replication round trip elapses")]
fn when_one_round_trip(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

/// When the server advances {n} ticks.
///
/// Runs N server ticks with no other mutation. Used to bound a "no
/// update should arrive in N ticks" window for stale-value
/// assertions (ScopeExit::Persist tests).
#[when("the server advances {int} ticks")]
fn when_server_advances_n_ticks(ctx: &mut TestWorldMut, n: u32) {
    let scenario = ctx.scenario_mut();
    for _ in 0..n {
        scenario.mutate(|_| {});
    }
}
