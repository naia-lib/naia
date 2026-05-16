//! C.2 gate: `parallel_send_matches_serial`
//!
//! Verifies that `WorldServer::send_all_packets` produces correct per-user
//! outcomes when executed under different rayon thread counts (1, 2, 4, 8).
//! Running under RAYON_NUM_THREADS=1 is equivalent to the serial baseline;
//! all thread counts must converge to identical component values on every client.
//!
//! Each of the four sub-runs is fully independent (fresh Scenario, clock reset).
//! The guarantee: the parallel Phase 3B refactor is transparent to clients.

#![allow(unused_imports)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::{ReplicationConfig, ServerConfig};

use naia_test_harness::{
    protocol, Auth, ClientKey, EntityKey, Position, Scenario, ServerAuthEvent, ServerConnectEvent,
};

mod _helpers;
use _helpers::client_connect;

const NUM_CLIENTS: usize = 8;

/// Run a complete spawn+mutate+replicate scenario with `num_threads` rayon
/// threads powering the server's `send_all_packets` call.
///
/// Asserts that all clients receive the correct component values after both the
/// initial replication round and a mutation round.
fn run_with_thread_count(num_threads: usize) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .expect("rayon pool build failed");

    let mut scenario = Scenario::new(); // resets TestClock to 0
    let proto = protocol();

    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    scenario.server_start(ServerConfig::default(), proto.clone());

    // Create a shared room so entities are visible to all users.
    let room_key = scenario.mutate(|mctx| mctx.server(|server| server.create_room().key()));

    // Connect NUM_CLIENTS clients.
    let client_keys: Vec<ClientKey> = (0..NUM_CLIENTS)
        .map(|i| {
            client_connect(
                &mut scenario,
                &room_key,
                &format!("client_{}", i),
                Auth::new(&format!("user_{}", i), "password"),
                client_config.clone(),
                proto.clone(),
            )
        })
        .collect();

    // Spawn one entity per client index, each with a distinct initial Position.
    let entity_keys: Vec<EntityKey> = (0..NUM_CLIENTS)
        .map(|i| {
            let (ek, _) = scenario.mutate(|mctx| {
                mctx.server(|server| {
                    server.spawn(|mut e| {
                        e.configure_replication(ReplicationConfig::public())
                            .insert_component(Position::new(i as f32, 0.0))
                            .enter_room(&room_key);
                    })
                })
            });
            ek
        })
        .collect();

    // ---- Initial replication: every client must see every entity ----
    pool.install(|| {
        scenario.expect(|ctx| {
            let all_seen = entity_keys.iter().all(|ek| {
                client_keys
                    .iter()
                    .all(|ck| ctx.client(*ck, |c| c.has_entity(ek)))
            });
            all_seen.then_some(())
        })
    });

    // Verify initial Position values on every client.
    pool.install(|| {
        scenario.expect(|ctx| {
            let all_correct = entity_keys.iter().enumerate().all(|(i, ek)| {
                client_keys.iter().all(|ck| {
                    ctx.client(*ck, |c| {
                        c.entity(ek)
                            .and_then(|e| e.component::<Position>().map(|p| (*p.x, *p.y)))
                            .map(|(x, y)| (x - i as f32).abs() < f32::EPSILON && y == 0.0)
                            .unwrap_or(false)
                    })
                })
            });
            all_correct.then_some(())
        })
    });

    // ---- Mutation round: give each entity a new, distinct Position ----
    for (i, ek) in entity_keys.iter().enumerate() {
        scenario.mutate(|mctx| {
            mctx.server(|server| {
                if let Some(mut entity) = server.entity_mut(ek) {
                    if let Some(mut pos) = entity.component::<Position>() {
                        *pos.x = (i as f32) * 10.0 + 1.0;
                        *pos.y = (i as f32) * 10.0 + 2.0;
                    }
                }
            });
        });
    }

    // ---- Verify updated values replicated to ALL clients ----
    pool.install(|| {
        scenario.expect(|ctx| {
            let all_updated = entity_keys.iter().enumerate().all(|(i, ek)| {
                let expected_x = (i as f32) * 10.0 + 1.0;
                let expected_y = (i as f32) * 10.0 + 2.0;
                client_keys.iter().all(|ck| {
                    ctx.client(*ck, |c| {
                        c.entity(ek)
                            .and_then(|e| e.component::<Position>().map(|p| (*p.x, *p.y)))
                            .map(|(x, y)| {
                                (x - expected_x).abs() < f32::EPSILON
                                    && (y - expected_y).abs() < f32::EPSILON
                            })
                            .unwrap_or(false)
                    })
                })
            });
            all_updated.then_some(())
        })
    });
}

/// C.2 gate — parallel_send_matches_serial
///
/// Run the same replication scenario under 1, 2, 4, and 8 rayon threads.
/// Each run is an independent fresh scenario with its own TestClock reset.
/// The invariant: per-user packet content is correct regardless of thread count.
///
/// Also runnable from CI via RAYON_NUM_THREADS env var:
///   RAYON_NUM_THREADS=1 cargo test parallel_send_matches_serial
///   RAYON_NUM_THREADS=8 cargo test parallel_send_matches_serial
#[test]
fn parallel_send_matches_serial() {
    run_with_thread_count(1);
    run_with_thread_count(2);
    run_with_thread_count(4);
    run_with_thread_count(8);
}
