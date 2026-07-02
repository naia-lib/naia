//! Per-user send-output oracle: `parallel_send_matches_serial`
//!
//! Verifies that `WorldServer::send_all_packets` produces correct per-user
//! component values for every client across a spawn + mutate + replicate cycle
//! (8 clients, 8 entities, a distinct Position per entity). It is the oracle
//! referenced by `send_state.rs` for the per-user Iris send path.
//!
//! History: this test used to run the scenario four times inside
//! `rayon::ThreadPool::install` (1/2/4/8 threads) to check the server's send
//! was thread-count-agnostic. That dimension is now vestigial — the per-user
//! `build_all_packets` loop is deliberately SERIAL (MISSION_CAPACITY
//! 2026-05-24; the rayon `into_par_iter` was measured slower and removed), so
//! the pool exercised no parallelism in the send. Worse, running the scenario
//! tick loop on a rayon worker thread tripped naia's THREAD-LOCAL `TestClock`:
//! setup ran on the main thread, the tick loop on a worker with its own fresh
//! clock, and the setup↔sim clock discontinuity corrupted one client's
//! time-sync (stranding its updates). The scenario now runs on a single
//! thread; `Scenario::tick` additionally guards against the cross-thread clock
//! footgun (see `Scenario::assert_clock_thread`).

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

/// Run a complete spawn + mutate + replicate scenario and assert every client
/// converges to the correct per-entity Position after both the initial
/// replication round and a mutation round.
fn run_replication_oracle() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident); // resets TestClock to 0
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
    scenario.expect(|ctx| {
        let all_seen = entity_keys.iter().all(|ek| {
            client_keys
                .iter()
                .all(|ck| ctx.client(*ck, |c| c.has_entity(ek)))
        });
        all_seen.then_some(())
    });

    // Verify initial Position values on every client.
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
    });
}

/// Oracle — `parallel_send_matches_serial`
///
/// The (serial) per-user Iris send path must deliver correct component values
/// to every client. Single-threaded: the scenario owns the thread-local
/// `TestClock` for its whole lifetime.
#[test]
fn parallel_send_matches_serial() {
    run_replication_oracle();
}
