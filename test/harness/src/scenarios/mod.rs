//! Standalone scenario functions for golden-trace capture.
//!
//! Each function runs a complete protocol sequence with wire-trace capture
//! enabled and returns the captured [`Trace`]. Used by `naia_spec_tool traces
//! record/check` to establish and verify golden wire-trace baselines.
//!
//! The traces are captured **after** the handshake completes, so they contain
//! only gameplay-level replication traffic — the invariant that Phases 2 and 3
//! must preserve byte-for-byte.

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::{ReplicationConfig as ServerReplicationConfig, ServerConfig};
use naia_shared::EntityAuthStatus;

use crate::harness::{
    ClientConnectEvent, EntityKey, Scenario, Trace, TrackedClientEvent, TrackedServerEvent,
};
use crate::harness::{ServerAuthEvent, ServerConnectEvent};
use crate::test_protocol::{protocol, Auth, Position};

// ============================================================================
// Shared connection helper
// ============================================================================

/// Connect one client through the full handshake, add to the scenario's last
/// room, and return its key. Trace capture must be enabled by the caller.
fn connect_one_client(scenario: &mut Scenario) -> crate::harness::ClientKey {
    let mut cfg = ClientConfig::default();
    cfg.send_handshake_interval = Duration::from_millis(0);
    cfg.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start("Client", Auth::new("user", "password"), cfg, protocol());

    scenario.expect(|ctx| {
        ctx.server(|s| {
            if let Some((k, _)) = s.read_event::<ServerAuthEvent<Auth>>() {
                if k == client_key {
                    return Some(k);
                }
            }
            None
        })
    });

    scenario.mutate(|ctx| {
        ctx.server(|s| s.accept_connection(&client_key));
    });

    scenario.expect(|ctx| {
        ctx.server(|s| {
            if let Some(k) = s.read_event::<ServerConnectEvent>() {
                if k == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);

    let room_key = scenario.last_room();
    scenario.mutate(|ctx| {
        ctx.server(|s| {
            s.room_mut(&room_key).expect("room exists").add_user(&client_key);
        });
    });

    scenario.expect(|ctx| ctx.client(client_key, |c| c.read_event::<ClientConnectEvent>()));
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);
    scenario.allow_flexible_next();

    client_key
}

// ============================================================================
// Contract 06 — scope entry
// ============================================================================

/// Golden trace for contract 06 — entity scope entry.
///
/// Captures the replication packets for a server-owned entity entering scope
/// for a single connected client (spawn + component delivery). Representative
/// of the scope-change propagation path that Phases 2 and 3 must preserve.
pub fn contract06_scope_entry() -> Trace {
    let mut scenario = Scenario::new();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|ctx| ctx.server(|s| s.make_room().key()));
    scenario.set_last_room(room_key);

    let client_key = connect_one_client(&mut scenario);

    // Capture only post-handshake gameplay traffic
    scenario.enable_trace_capture();

    let (entity_key, ()): (EntityKey, ()) = scenario.mutate(|ctx| {
        ctx.server(|s| {
            s.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0)).enter_room(&room_key);
            })
        })
    });

    // Wait for entity to appear on client (scope-entry spawn delivered)
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            if c.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });

    scenario.take_trace()
}

// ============================================================================
// Contract 07 — component update replication
// ============================================================================

/// Golden trace for contract 07 — component update replication.
///
/// Captures the wire packets for a Position component update after the entity
/// is already in scope. Representative of the update-dispatch path that Phase 3
/// must preserve.
pub fn contract07_component_update() -> Trace {
    let mut scenario = Scenario::new();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|ctx| ctx.server(|s| s.make_room().key()));
    scenario.set_last_room(room_key);

    let client_key = connect_one_client(&mut scenario);

    let (entity_key, ()): (EntityKey, ()) = scenario.mutate(|ctx| {
        ctx.server(|s| {
            s.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0)).enter_room(&room_key);
            })
        })
    });

    // Wait for entity to appear on client before starting trace
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            if c.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });

    // Capture starting from the update — handshake and spawn already delivered
    scenario.enable_trace_capture();

    // Mutate Position on server (component() on ServerEntityMut returns ReplicaMutWrapper)
    scenario.mutate(|ctx| {
        ctx.server(|s| {
            if let Some(mut entity) = s.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = 42.0;
                    *pos.y = 42.0;
                }
            }
        });
    });

    // Wait for client to observe the updated values
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let entity = c.entity(&entity_key)?;
            let pos = entity.component::<Position>()?;
            if *pos.x == 42.0 && *pos.y == 42.0 {
                Some(())
            } else {
                None
            }
        })
    });

    scenario.take_trace()
}

// ============================================================================
// Contract 10 — delegation authority grant
// ============================================================================

/// Golden trace for contract 10 — delegated entity authority grant.
///
/// Captures the wire packets for: delegated entity scope entry (Available
/// status delivered) + client authority request + server authority grant
/// (Granted status delivered). Representative of the delegation path.
pub fn contract10_delegation_grant() -> Trace {
    let mut scenario = Scenario::new();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|ctx| ctx.server(|s| s.make_room().key()));
    scenario.set_last_room(room_key);

    let client_key = connect_one_client(&mut scenario);

    let (entity_key, ()): (EntityKey, ()) = scenario.mutate(|ctx| {
        ctx.server(|s| {
            s.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ServerReplicationConfig::delegated())
                    .enter_room(&room_key);
            })
        })
    });

    scenario.mutate(|ctx| {
        ctx.server(|s| {
            if let Some(mut scope) = s.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });

    // Wait for entity to appear with Available authority status
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            match c.entity(&entity_key).and_then(|e| e.authority()) {
                Some(EntityAuthStatus::Available) => Some(()),
                _ => None,
            }
        })
    });

    // Capture starting from the authority request
    scenario.enable_trace_capture();

    // Client requests authority
    scenario.mutate(|ctx| {
        ctx.client(client_key, |c| {
            if let Some(mut entity) = c.entity_mut(&entity_key) {
                entity
                    .request_authority()
                    .expect("request_authority should succeed for in-scope delegated entity");
            }
        });
    });

    // Wait for Granted status
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            match c.entity(&entity_key).and_then(|e| e.authority()) {
                Some(EntityAuthStatus::Granted) => Some(()),
                _ => None,
            }
        })
    });

    scenario.take_trace()
}
