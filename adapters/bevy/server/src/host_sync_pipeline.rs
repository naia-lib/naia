//! Pipeline-mode equivalent of `world_to_host_sync` — Phase 1 of the
//! `Plugin::sim_integration` (Iris 2) host-side wiring.
//!
//! Today's [`crate::systems::world_to_host_sync`] takes
//! `ResMut<ServerImpl>` and drains `Messages<HostSyncEvent>` into naia's
//! `insert_component_worldless` / `remove_component_worldless` /
//! `despawn_entity_worldless` machinery. Under the three-handle pipeline
//! (`Plugin::types_and_sets_only` / `Plugin::sim_integration`)
//! `ServerImpl` does not exist; the equivalent drain runs against the
//! three pipeline handles directly.
//!
//! MISSION_USER_ONLY_SEES_SIM Phase D.3b.2 (2026-05-19): this helper no
//! longer reassembles a `WorldServer` via `run_with_world_server`. The
//! `is_listening` guard reads `SendHandle::is_listening`, the auth gate
//! reads `CoordHandle::entity_authority_status`, and the insert / remove /
//! despawn ops route through the `SendHandle::*_worldless` methods (the
//! despawn borrows `&mut coord.state` for its Coord-side priority/room
//! cleanup). Each worldless method mirrors its `WorldServer` namesake
//! field-for-field, so the wire output is byte-identical to the prior
//! reassembly path. This is the naia surface that retires cyberlith's
//! `world_to_host_sync_pipeline` `run_with_naia_server` reassembly
//! (Phase E.6).
//!
//! This is a sibling helper to [`crate::apply_receive_output_pipeline`]
//! and follows the same calling convention: the caller passes the three
//! handles + the bevy world by reference; this function does the
//! handle-take / WorldServer-rebuild / drain / re-split internally.
//!
//! See `SPEC_IRIS_2_NAIA.md` §1.4 (cyberlith repo) for the design
//! rationale.

use std::ops::DerefMut;

use bevy_ecs::{entity::Entity, message::Messages, world::World};

use naia_bevy_shared::{EntityAuthStatus, HostSyncEvent, WorldMutType, WorldProxyMut};
use naia_server::pipeline_actors::CoordHandle;
use naia_server::{RecvHandle, SendHandle};

/// Drain `Messages<HostSyncEvent>` against the three pipeline handles
/// — pipeline-mode equivalent of [`crate::systems::world_to_host_sync`].
///
/// Mirrors the byte-for-byte semantics of `world_to_host_sync` (auth
/// gating, insert / remove / despawn dispatch, error tolerance for
/// missing components on insert) but routes through
/// [`run_with_world_server`] instead of `ResMut<ServerImpl>`.
///
/// Calling pattern (cyberlith Sim main schedule). Under
/// `Plugin::sim_integration_full`, `RecvHandleRes` / `SendHandleRes`
/// wrap shared park-window slots: the workers must be parked (via
/// [`crate::PluginInternalState::park_workers`]) before taking them, and
/// unparked after returning them. `CoordHandleRes` is a plain `Option`
/// (coord lives only on main):
/// ```ignore
/// fn sim_to_host_sync(world: &mut World) {
///     let state = world.resource::<PluginInternalState>();
///     state.park_workers();
///
///     let mut coord = world.resource_mut::<CoordHandleRes>().0.take().unwrap();
///     let recv_slot = world.resource::<RecvHandleRes>().0.clone();
///     let send_slot = world.resource::<SendHandleRes>().0.clone();
///     let recv = recv_slot.lock().take().unwrap();
///     let send = send_slot.lock().take().unwrap();
///
///     let (coord, recv, send) =
///         drain_host_sync_into_pipeline(world, coord, recv, send);
///
///     world.resource_mut::<CoordHandleRes>().0 = Some(coord);
///     *recv_slot.lock() = Some(recv);
///     *send_slot.lock() = Some(send);
///
///     world.resource::<PluginInternalState>().unpark_workers();
/// }
/// ```
///
/// Returns the three handles re-split so the caller can park them back
/// into their resources. Matches [`run_with_world_server`]'s
/// take-then-return convention.
///
/// If no `HostSyncEvent`s are pending the function returns the handles
/// unchanged without rebuilding `WorldServer` (zero-cost no-op path).
pub fn drain_host_sync_into_pipeline(
    world: &mut World,
    mut coord: CoordHandle<Entity>,
    recv: RecvHandle<Entity>,
    mut send: SendHandle<Entity>,
) -> (CoordHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>) {
    // Drain the message queue first; if empty, skip all handle work.
    let host_component_events: Vec<HostSyncEvent> = world
        .get_resource_mut::<Messages<HostSyncEvent>>()
        .map(|mut reader| reader.drain().collect())
        .unwrap_or_default();
    if host_component_events.is_empty() {
        return (coord, recv, send);
    }

    // MISSION_USER_ONLY_SEES_SIM Phase D.3b.2 (2026-05-19) — handle-direct
    // drain. No `run_with_world_server` reassembly: queries route through
    // `SendHandle::is_listening` + `CoordHandle::entity_authority_status`,
    // mutations through the `SendHandle::*_worldless` ops (despawn borrows
    // `&mut coord.state`). Byte-identical to the prior `WorldServer` path
    // — the worldless methods mirror `WorldServer::*` field-for-field.

    // Skip drain entirely while not listening — matches today's
    // `world_to_host_sync` guard on `server.is_listening()`.
    if !send.is_listening() {
        return (coord, recv, send);
    }
    for event in host_component_events {
        match event {
            HostSyncEvent::Insert(_host_id, entity, component_kind) => {
                if coord.entity_authority_status(&entity)
                    == Some(EntityAuthStatus::Denied)
                {
                    // Client holds auth — skip (client driver will apply
                    // the insert via the receive path).
                    continue;
                }
                let mut world_proxy = world.proxy_mut();
                let Some(mut component_mut) =
                    world_proxy.component_mut_of_kind(&entity, &component_kind)
                else {
                    // Component already removed between emission and drain
                    // — same tolerant behavior as the non-pipelined path.
                    continue;
                };
                send.insert_component_worldless(
                    &entity,
                    DerefMut::deref_mut(&mut component_mut),
                );
            }
            HostSyncEvent::Remove(_host_id, entity, component_kind) => {
                if coord.entity_authority_status(&entity)
                    == Some(EntityAuthStatus::Denied)
                {
                    continue;
                }
                send.remove_component_worldless(&entity, &component_kind);
            }
            HostSyncEvent::Despawn(_host_id, entity) => {
                if coord.entity_authority_status(&entity)
                    == Some(EntityAuthStatus::Denied)
                {
                    continue;
                }
                send.despawn_entity_worldless(&mut coord.state, &entity);
            }
        }
    }

    (coord, recv, send)
}
