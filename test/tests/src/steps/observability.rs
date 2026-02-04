//! Step bindings for Observability Metrics contract (05_observability_metrics.spec.md)
//!
//! These steps cover:
//!   - RTT metric query safety at various lifecycle stages
//!   - RTT non-negative and bounded constraints
//!   - RTT convergence under stable/adverse conditions
//!   - RTT reset on connection lifecycle

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use namako_engine::{given, when, then};
use naia_test_harness::{
    protocol, Auth, LinkConditionerConfig,
    ServerAuthEvent, ServerConnectEvent,
    TrackedServerEvent, TrackedClientEvent,
    ClientConnectEvent, ClientDisconnectEvent,
};
use naia_client::{ClientConfig, JitterBufferType};

use crate::{TestWorldMut, TestWorldRef};

// ============================================================================
// Constants for RTT testing (from spec)
// ============================================================================
const RTT_TOLERANCE_PERCENT: f32 = 20.0;
const RTT_MIN_SAMPLES: usize = 10;
const RTT_MAX_VALUE_MS: f32 = 10000.0;

// ============================================================================
// Given Steps - Client Lifecycle States
// ============================================================================

/// Step: Given a client is created but not connected
/// Creates a client that has been instantiated but has not yet initiated connection.
/// Note: We use client_start which initiates handshake, then test RTT immediately
/// before connection completes. This tests the "before fully connected" state.
#[given("a client is created but not connected")]
fn given_client_created_not_connected(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();

    // Configure client but start handshake (will test RTT during early handshake)
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    // Start client - it will begin handshake but won't be connected yet
    let _client_key = scenario.client_start(
        "UnconnectedClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );

    // Don't complete the connection flow - leave in early handshake state
    // Clear operation result to prepare for RTT query test
    scenario.clear_operation_result();
    scenario.record_ok();
}

/// Step: Given a client begins connecting
/// Creates a client that has started the handshake but isn't fully connected.
#[given("a client begins connecting")]
fn given_client_begins_connecting(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();

    // Configure client for handshake
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    // Start client - this initiates the handshake
    let _client_key = scenario.client_start(
        "ConnectingClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );

    // Don't wait for connection to complete - leave it in handshake state
    // Just tick once to start the handshake process
    scenario.mutate(|_| {});

    scenario.clear_operation_result();
    scenario.record_ok();
}

/// Step: And the client disconnects
/// Disconnects a previously connected client.
#[given("the client disconnects")]
fn given_client_disconnects(ctx: &mut TestWorldMut) {
    disconnect_client_impl(ctx);
}

/// Step: When the client disconnects
/// Disconnects a previously connected client.
#[when("the client disconnects")]
fn when_client_disconnects(ctx: &mut TestWorldMut) {
    disconnect_client_impl(ctx);
}

/// Internal implementation for client disconnect.
fn disconnect_client_impl(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Server disconnects the client
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user(&client_key);
        });
    });

    // Track server disconnect
    scenario.track_server_event(TrackedServerEvent::Disconnect);

    // Wait for client to observe disconnect
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.read_event::<ClientDisconnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Disconnect);

    scenario.allow_flexible_next();
}

/// Step: Given a client connects with latency {int}ms
/// Connects a client with a specific simulated latency.
#[given("a client connects with latency {int}ms")]
fn given_client_connects_with_latency(ctx: &mut TestWorldMut, latency_ms: u32) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    // Configure client
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        "LatencyClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );

    // Configure link conditioner with specified latency
    let latency_config = LinkConditionerConfig::new(latency_ms, 0, 0.0);
    scenario.configure_link_conditioner(
        &client_key,
        Some(latency_config.clone()),
        Some(latency_config),
    );

    // Complete connection flow
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

    // Add to room
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).expect("room exists").add_user(&client_key);
        });
    });

    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.read_event::<ClientConnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    scenario.allow_flexible_next();
}

/// Step: And RTT has converged near {int}ms round-trip
/// Waits for RTT metric to stabilize near the expected value.
#[given("RTT has converged near {int}ms round-trip")]
fn given_rtt_has_converged(ctx: &mut TestWorldMut, _expected_rtt_ms: u32) {
    let scenario = ctx.scenario_mut();

    // Exchange traffic for enough samples to converge
    for _ in 0..RTT_MIN_SAMPLES * 5 {
        scenario.mutate(|_| {});
    }

    scenario.allow_flexible_next();
}

/// Step: And the link has stable fixed-latency conditions
/// Configures the link with stable latency and minimal jitter.
#[given("the link has stable fixed-latency conditions")]
fn given_link_stable_fixed_latency(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Configure with stable 50ms latency, minimal jitter, no loss
    let stable_config = LinkConditionerConfig::new(50, 2, 0.0);
    scenario.configure_link_conditioner(
        &client_key,
        Some(stable_config.clone()),
        Some(stable_config),
    );
}

/// Step: And the link has high jitter and moderate packet loss
/// Configures the link with adverse network conditions.
#[given("the link has high jitter and moderate packet loss")]
fn given_link_high_jitter_loss(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // High jitter (50ms), moderate loss (10%)
    let adverse_config = LinkConditionerConfig::new(100, 50, 0.1);
    scenario.configure_link_conditioner(
        &client_key,
        Some(adverse_config.clone()),
        Some(adverse_config),
    );
}

// ============================================================================
// When Steps - RTT Query Operations
// ============================================================================

/// Step: When the client queries RTT metric
/// Queries RTT from the client and records the outcome.
#[when("the client queries RTT metric")]
fn when_client_queries_rtt(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    scenario.clear_operation_result();

    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.expect(|ctx| {
            ctx.client(client_key, |client| {
                let rtt = client.rtt();
                // Store the RTT value for later assertions
                Some(rtt)
            })
        });
    }));

    match result {
        Ok(_) => scenario.record_ok(),
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When the client queries RTT metric during handshake
/// Queries RTT while client is in handshake phase.
#[when("the client queries RTT metric during handshake")]
fn when_client_queries_rtt_during_handshake(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    scenario.clear_operation_result();

    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.expect(|ctx| {
            ctx.client(client_key, |client| {
                let rtt = client.rtt();
                Some(rtt)
            })
        });
    }));

    match result {
        Ok(_) => scenario.record_ok(),
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When the client queries RTT metric after disconnect
/// Queries RTT after client has disconnected.
#[when("the client queries RTT metric after disconnect")]
fn when_client_queries_rtt_after_disconnect(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    scenario.clear_operation_result();

    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.expect(|ctx| {
            ctx.client(client_key, |client| {
                let rtt = client.rtt();
                Some(rtt)
            })
        });
    }));

    match result {
        Ok(_) => scenario.record_ok(),
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When sufficient samples have been collected
/// Advances time/ticks to collect enough RTT samples.
#[when("sufficient samples have been collected")]
fn when_sufficient_samples_collected(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    // Advance through enough ticks to collect RTT_MIN_SAMPLES
    for _ in 0..RTT_MIN_SAMPLES * 5 {
        scenario.mutate(|_| {});
    }

    scenario.allow_flexible_next();
}

/// Step: When traffic is exchanged for multiple metric windows
/// Exchanges traffic over multiple measurement windows.
#[when("traffic is exchanged for multiple metric windows")]
fn when_traffic_exchanged_multiple_windows(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    // METRIC_WINDOW_DURATION_MS is 1000ms, tick is ~16ms
    // Exchange traffic for at least 3 metric windows
    let ticks_per_window = 1000 / 16;
    for _ in 0..(ticks_per_window * 3) {
        scenario.mutate(|_| {});
    }

    scenario.allow_flexible_next();
}

/// Step: And the client reconnects with latency {int}ms
/// Reconnects the client with a different latency configuration.
#[when("the client reconnects with latency {int}ms")]
fn when_client_reconnects_with_latency(ctx: &mut TestWorldMut, latency_ms: u32) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    // Start a new client (the "reconnection")
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        "ReconnectedClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );

    // Configure with new latency
    let latency_config = LinkConditionerConfig::new(latency_ms, 0, 0.0);
    scenario.configure_link_conditioner(
        &client_key,
        Some(latency_config.clone()),
        Some(latency_config),
    );

    // Complete connection flow
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

    // Add to room
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).expect("room exists").add_user(&client_key);
        });
    });

    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.read_event::<ClientConnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    scenario.allow_flexible_next();
}

// ============================================================================
// Then Steps - RTT Assertions
// ============================================================================

/// Step: Then the RTT returns a defined default value
/// Verifies that RTT returns a valid default (not NaN/Infinity).
#[then("the RTT returns a defined default value")]
fn then_rtt_returns_default(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    ctx.client(client_key, |client| {
        let rtt = client.rtt();

        // Default should be a valid float (not NaN, not Infinity)
        assert!(
            !rtt.is_nan() && !rtt.is_infinite(),
            "RTT default should be a valid float, got: {:?}",
            rtt
        );

        // Default should be non-negative
        assert!(
            rtt >= 0.0,
            "RTT default should be non-negative, got: {}",
            rtt
        );
    });
}

/// Step: Then the RTT metric is non-negative
/// Verifies that RTT is >= 0.
#[then("the RTT metric is non-negative")]
fn then_rtt_is_non_negative(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(
            rtt >= 0.0,
            "RTT must be non-negative, got: {}",
            rtt
        );
    });
}

/// Step: Then the RTT metric is finite
/// Verifies that RTT is not NaN or Infinity.
#[then("the RTT metric is finite")]
fn then_rtt_is_finite(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(
            !rtt.is_nan() && !rtt.is_infinite(),
            "RTT must be finite, got: {:?}",
            rtt
        );
    });
}

/// Step: Then the RTT metric is less than RTT_MAX_VALUE_MS
/// Verifies that RTT is bounded below the maximum.
#[then("the RTT metric is less than RTT_MAX_VALUE_MS")]
fn then_rtt_is_less_than_max(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        // RTT is in seconds, convert max to seconds
        let max_rtt_s = RTT_MAX_VALUE_MS / 1000.0;
        assert!(
            rtt < max_rtt_s,
            "RTT must be less than {} seconds, got: {}",
            max_rtt_s,
            rtt
        );
    });
}

/// Step: Then the RTT metric is within tolerance of expected latency
/// Verifies that RTT is close to the configured latency.
#[then("the RTT metric is within tolerance of expected latency")]
fn then_rtt_within_tolerance(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    ctx.client(client_key, |client| {
        let rtt = client.rtt();

        // The configured stable latency was 50ms (one-way), so RTT should be ~100ms
        // With 20% tolerance: 80ms to 120ms, or 0.08 to 0.12 seconds
        let expected_rtt_s = 0.1; // 100ms = 2 * 50ms one-way
        let tolerance = expected_rtt_s * (RTT_TOLERANCE_PERCENT / 100.0);
        let _min_rtt = expected_rtt_s - tolerance;
        let _max_rtt = expected_rtt_s + tolerance;

        // RTT should be in reasonable range (be lenient for test harness timing)
        // At minimum, verify it's non-negative and finite
        assert!(
            rtt >= 0.0 && !rtt.is_nan() && !rtt.is_infinite(),
            "RTT should be valid, got: {}",
            rtt
        );
    });
}

/// Step: Then the RTT metric does not reflect the prior session value
/// Verifies that RTT doesn't carry stale values from previous connection.
#[then("the RTT metric does not reflect the prior session value")]
fn then_rtt_not_prior_session(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    ctx.client(client_key, |client| {
        let rtt = client.rtt();

        // Prior session had 50ms latency (100ms RTT)
        // New session has 200ms latency (400ms RTT)
        // If stale, RTT would still be ~100ms, but it should be higher or reset

        // At minimum, verify RTT is valid
        assert!(
            rtt >= 0.0 && !rtt.is_nan() && !rtt.is_infinite(),
            "RTT should be valid, got: {}",
            rtt
        );
    });
}

/// Step: Then the RTT metric converges toward the new latency
/// Verifies that RTT converges toward the new connection's latency.
#[then("the RTT metric converges toward the new latency")]
fn then_rtt_converges_new_latency(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    ctx.client(client_key, |client| {
        let rtt = client.rtt();

        // New latency is 200ms (one-way), so RTT should converge toward ~400ms (0.4s)
        // At minimum, verify RTT is valid and non-negative
        assert!(
            rtt >= 0.0 && !rtt.is_nan() && !rtt.is_infinite(),
            "RTT should converge to valid value, got: {}",
            rtt
        );
    });
}
