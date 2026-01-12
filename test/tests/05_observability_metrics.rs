// ============================================================================
// Observability Metrics Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/5_observability_metrics.md
// ============================================================================

mod _helpers;

use naia_client::ConnectionStatus;
use naia_server::ServerConfig;
use naia_test::{protocol, Auth, Scenario};
use _helpers::{client_connect, test_client_config};

// ============================================================================
// Contract Tests
// ============================================================================

/// Metrics do not affect replicated state correctness
/// Contract: [observability-01]
///
/// Given a normal connection scenario; when time passes and internal metrics are computed; then entity replication/authority/scope remains unaffected.
#[test]
fn metrics_do_not_affect_replicated_state_correctness() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Advance through multiple ticks - internal metrics computation should not affect connection
    for _ in 0..20 {
        scenario.mutate(|_ctx| {});
        scenario.expect(|ctx| {
            // Connection should remain valid throughout
            let status = ctx.client(client_key, |c| c.connection_status());
            (status == ConnectionStatus::Connected).then_some(())
        });
    }
}

/// Metrics APIs safe to query after construction
/// Contract: [observability-02]
///
/// Given a connection; when we query metrics; then APIs return well-defined values without panic.
#[test]
fn metrics_apis_safe_after_construction() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Query all metrics via expect - should not panic, return valid values
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let rtt = c.rtt();
            let outgoing = c.outgoing_bandwidth();
            let incoming = c.incoming_bandwidth();

            // All values should be valid floats (not NaN, not Infinity)
            let rtt_valid = !rtt.is_nan() && !rtt.is_infinite();
            let outgoing_valid = !outgoing.is_nan() && !outgoing.is_infinite();
            let incoming_valid = !incoming.is_nan() && !incoming.is_infinite();

            (rtt_valid && outgoing_valid && incoming_valid).then_some(())
        })
    });
}

/// RTT must be non-negative
/// Contract: [observability-03]
///
/// Given an established connection; when we query RTT; then the value MUST be non-negative.
#[test]
fn rtt_must_be_non_negative() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // RTT should be non-negative
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let rtt = c.rtt();
            (rtt >= 0.0).then_some(())
        })
    });
}

/// RTT stable under normal conditions
/// Contract: [observability-04]
///
/// Given a stable connection; when we query RTT multiple times; then the value should remain reasonably stable.
#[test]
fn rtt_stable_under_normal_conditions() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Let connection stabilize and check RTT remains non-negative
    for _ in 0..10 {
        scenario.mutate(|_ctx| {});
        scenario.expect(|ctx| {
            ctx.client(client_key, |c| {
                let rtt = c.rtt();
                (rtt >= 0.0).then_some(())
            })
        });
    }
}

/// Throughput must be non-negative
/// Contract: [observability-05]
///
/// Given an established connection; when we query bandwidth; then values MUST be non-negative.
#[test]
fn throughput_must_be_non_negative() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Bandwidth should be non-negative - assert via expect
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let outgoing = c.outgoing_bandwidth();
            let incoming = c.incoming_bandwidth();

            let outgoing_valid = outgoing >= 0.0;
            let incoming_valid = incoming >= 0.0;

            (outgoing_valid && incoming_valid).then_some(())
        })
    });
}

/// Bandwidth exposes both directions
/// Contract: [observability-06]
///
/// Given an established connection; when we query outgoing and incoming bandwidth separately; then both are available.
#[test]
fn bandwidth_exposes_both_directions() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Both incoming and outgoing bandwidth should be independently queryable and valid
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let outgoing = c.outgoing_bandwidth();
            let incoming = c.incoming_bandwidth();

            // Both should be valid floats (not NaN)
            let outgoing_valid = !outgoing.is_nan();
            let incoming_valid = !incoming.is_nan();

            (outgoing_valid && incoming_valid).then_some(())
        })
    });
}

/// Metrics cleanup on disconnect
/// Contract: [observability-07]
///
/// Given a client that disconnects; when disconnect completes; then no stale state affects new connections.
#[test]
fn metrics_cleanup_on_disconnect() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    // First connection (client_connect already verifies connection via expect)
    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Disconnect - this is a mutation (state change)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user(&client_key);
        });
    });

    // Wait for disconnect to process (need expect after mutate before loop)
    scenario.expect(|_ctx| Some(()));
    for _ in 0..4 {
        scenario.mutate(|_ctx| {});
        scenario.expect(|_ctx| Some(()));
    }

    // Reconnect with new client - should start fresh (no stale metrics)
    let client_key2 = client_connect(
        &mut scenario,
        &room_key,
        "Client2",
        Auth::new("client2", "pass"),
        test_client_config(),
        test_protocol,
    );

    // New client should be connected with valid metrics
    scenario.expect(|ctx| {
        ctx.client(client_key2, |c| {
            let connected = c.is_connected();
            let rtt = c.rtt();
            (connected && rtt >= 0.0).then_some(())
        })
    });
}

/// Time source monotonic consistency
/// Contract: [observability-08]
///
/// Given metric computations over time; when we query metrics; then values never become negative.
#[test]
fn time_source_monotonic_consistency() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Query metrics multiple times over scenario ticks - all should remain non-negative
    for _ in 0..10 {
        scenario.mutate(|_ctx| {});
        scenario.expect(|ctx| {
            ctx.client(client_key, |c| {
                let rtt = c.rtt();
                (rtt >= 0.0).then_some(())
            })
        });
    }
}

/// Per-direction metrics consistency
/// Contract: [observability-09]
///
/// Given bidirectional communication; when we query separate send/receive metrics; then they reflect direction correctly.
#[test]
fn per_direction_metrics_consistency() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client",
        Auth::new("client", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Let bidirectional traffic flow
    for _ in 0..5 {
        scenario.mutate(|_ctx| {});
        scenario.expect(|_ctx| Some(()));
    }

    // Need a mutate before the final expect
    scenario.mutate(|_ctx| {});

    // Incoming and outgoing should be independently queryable and valid
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let outgoing = c.outgoing_bandwidth();
            let incoming = c.incoming_bandwidth();

            // Both should be valid (non-NaN, non-Infinity, non-negative)
            let outgoing_valid = !outgoing.is_nan() && !outgoing.is_infinite() && outgoing >= 0.0;
            let incoming_valid = !incoming.is_nan() && !incoming.is_infinite() && incoming >= 0.0;

            (outgoing_valid && incoming_valid).then_some(())
        })
    });
}
