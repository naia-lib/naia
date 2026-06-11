//! MISSION_TICK_FLOOR Lever 3 regression gate (naia layer).
//!
//! Pins the correctness of the one-tick send *lag* + the prepared, self-contained
//! send plan used by the active send worker (`SendState::prepare_send_job` on
//! MAIN at the freeze point + `SendState::transmit_send_job` lagged).
//!
//! ## The bug this gates against
//!
//! With the lag, the server transmits the *previous* tick's frozen job (a
//! `SnapshotWorld` + a frozen `global_dirty` set) while live state advances.
//! naia clears a component's per-user diff mask at *send* time
//! (`entity_update_manager.rs::record_update_dense`). So for a component
//! dirtied on two CONSECUTIVE ticks (N-1 and N) that then stops:
//!   - tick N's lagged send transmits value@(N-1) and CLEARS the per-user mask;
//!   - tick N+1's lagged send finds the entity in `frozen(N)` but Phase 3A
//!     re-gates each entry on the now-clear live mask (`send_state.rs`
//!     `diff_mask_is_clear_dense`) and SKIPS it → value@N is dropped, with no
//!     NACK to trigger the unacked-replay ledger. The client is left
//!     permanently one mutation stale.
//!
//! ## Setup
//!
//! A SERVER-OWNED `public` `Position` entity observed by a client (no
//! prediction/rollback/FoW) — the cleanest observable; `parallel_send_matches_serial`
//! proves such an entity's value tracks server mutations. Connect + spawn +
//! initial replication go through the `Scenario` (which auto-sends normally);
//! the LAG is driven manually via `Scenario::with_server_world_mut` (stage the
//! mutation) + `Scenario::prepare_send_job` (build the frozen send plan AT the
//! freeze point — capturing each `DiffMask` and clearing the live mask) +
//! `Scenario::transmit_and_pump` (transmit the prepared plan a tick later, the
//! lagged send).
//!
//! The fix (`prepare_send_job` / `transmit_send_job`): the per-user send DECISION
//! — including the per-property `DiffMask` — is built and the live masks cleared
//! at the FREEZE point, so each tick's plan is self-contained and the lagged
//! transmit reads zero live per-user diff state.

#![allow(unused_imports)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::{ReplicationConfig, ServerConfig};
use naia_shared::{ComponentKind, Replicate, SnapshotWorld, WorldRefType};

use naia_test_harness::{protocol, Auth, ClientKey, EntityKey, Position, Scenario, TestEntity};

mod _helpers;
use _helpers::client_connect;

const V1: (f32, f32) = (10.0, 11.0);
const V2: (f32, f32) = (20.0, 22.0);

fn build_snap(entity: TestEntity, (x, y): (f32, f32)) -> SnapshotWorld<TestEntity> {
    let mut snap = SnapshotWorld::new();
    let boxed: Box<dyn Replicate> = Box::new(Position::new(x, y));
    snap.insert_component(entity, ComponentKind::of::<Position>(), boxed);
    snap
}

#[test]
fn lagged_send_delivers_final_consecutive_value() {
    let mut scenario = Scenario::new();
    let proto = protocol();

    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    scenario.server_start(ServerConfig::default(), proto.clone());
    let room_key = scenario.mutate(|mctx| mctx.server(|s| s.create_room().key()));
    let client_key: ClientKey = client_connect(
        &mut scenario,
        &room_key,
        "client",
        Auth::new("user", "password"),
        client_config,
        proto.clone(),
    );

    // Spawn a server-owned public Position entity in the room (auto-replicates).
    let (entity_key, _): (EntityKey, _) = scenario.mutate(|mctx| {
        mctx.server(|s| {
            s.spawn(|mut e| {
                e.configure_replication(ReplicationConfig::public())
                    .insert_component(Position::new(0.0, 0.0))
                    .enter_room(&room_key);
            })
        })
    });

    // Wait until the client observes the entity + its Position, then settle so
    // the spawn is acked and the per-user diff mask is clear (steady state).
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.entity(&entity_key)
                .and_then(|e| e.component::<Position>().map(|p| (*p.x, *p.y)))
        })
        .map(|_| ())
    });
    for _ in 0..20 {
        scenario.expect(|_| Some(()));
    }

    // Resolve the server-side world entity (single entity in this scenario).
    let server_entity = scenario.with_server_world_mut(|_s, world| {
        world
            .proxy()
            .entities()
            .into_iter()
            .next()
            .expect("server entity")
    });

    // ── The lag: two CONSECUTIVE mutations, then stop ───────────────────────
    // Each mutation is followed by a `prepare_send_job` AT the freeze point: it
    // captures the dirty `DiffMask` into a self-contained plan and clears the
    // live mask. The next mutation then re-dirties cleanly. No send happens yet.
    scenario.with_server_world_mut(|s, world| {
        if let Some(mut pos) = s
            .entity_mut(world.proxy_mut(), &server_entity)
            .component::<Position>()
        {
            *pos.x = V1.0;
            *pos.y = V1.1;
        }
    });
    let snap1 = build_snap(server_entity, V1);
    let plan1 = scenario.prepare_send_job();

    scenario.with_server_world_mut(|s, world| {
        if let Some(mut pos) = s
            .entity_mut(world.proxy_mut(), &server_entity)
            .component::<Position>()
        {
            *pos.x = V2.0;
            *pos.y = V2.1;
        }
    });
    let snap2 = build_snap(server_entity, V2);
    let plan2 = scenario.prepare_send_job();

    // Lagged transmit of the N-1 plan (sends V1), then the N plan (sends V2).
    // Each plan carries its own frozen DiffMask, so V2 is NOT dropped: the fix
    // captured + cleared masks at the freeze points above, not at transmit time.
    scenario.transmit_and_pump(snap1, plan1);
    scenario.transmit_and_pump(snap2, plan2);

    // Flush delivery (a few ticks for the one-tick send latency). The entity is
    // no longer dirty, so live sends re-send nothing for it — the client
    // converges to what actually went on the wire.
    for _ in 0..10 {
        scenario.expect(|_| Some(()));
    }
    let got = scenario.expect(|ctx| {
        Some(ctx.client(client_key, |c| {
            c.entity(&entity_key)
                .and_then(|e| e.component::<Position>().map(|p| (*p.x, *p.y)))
        }))
    });
    assert_eq!(
        got,
        Some(V2),
        "MISSION_TICK_FLOOR Lever 3: client must receive the FINAL value (V2) of a \
         component mutated on >=2 consecutive ticks then stopped. The one-tick send \
         lag dropped V2 (per-user diff mask cleared by the lagged send of V1; the \
         frozen-plan entry for V2 was then skipped on the clear mask). got={got:?}"
    );
}
