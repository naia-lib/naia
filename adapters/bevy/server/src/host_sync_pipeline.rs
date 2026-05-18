//! Pipeline-mode equivalent of `world_to_host_sync` — Phase 1 of the
//! `Plugin::sim_integration` (Iris 2) host-side wiring.
//!
//! Today's [`crate::systems::world_to_host_sync`] takes
//! `ResMut<ServerImpl>` and drains `Messages<HostSyncEvent>` into naia's
//! `insert_component_worldless` / `remove_component_worldless` /
//! `despawn_entity_worldless` machinery. Under the three-handle pipeline
//! (`Plugin::types_and_sets_only` / `Plugin::sim_integration`)
//! `ServerImpl` does not exist; the equivalent drain runs against the
//! three pipeline handles instead, briefly re-assembling them into a
//! `WorldServer` via [`run_with_world_server`].
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
use naia_server::pipeline_actors::{CoordHandle, run_with_world_server};
use naia_server::{RecvHandle, SendHandle};

/// Drain `Messages<HostSyncEvent>` against the three pipeline handles
/// — pipeline-mode equivalent of [`crate::systems::world_to_host_sync`].
///
/// Mirrors the byte-for-byte semantics of `world_to_host_sync` (auth
/// gating, insert / remove / despawn dispatch, error tolerance for
/// missing components on insert) but routes through
/// [`run_with_world_server`] instead of `ResMut<ServerImpl>`.
///
/// Calling pattern (cyberlith Sim main schedule):
/// ```ignore
/// fn sim_to_host_sync(world: &mut World) {
///     let mut coord = world.resource_mut::<CoordHandleRes>().0.take().unwrap();
///     let mut recv  = world.resource_mut::<RecvHandleRes>().0.take().unwrap();
///     let mut send  = world.resource_mut::<SendHandleRes>().0.take().unwrap();
///
///     let (coord, recv, send) =
///         drain_host_sync_into_pipeline(world, coord, recv, send);
///
///     world.resource_mut::<CoordHandleRes>().0 = Some(coord);
///     world.resource_mut::<RecvHandleRes>().0 = Some(recv);
///     world.resource_mut::<SendHandleRes>().0 = Some(send);
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
    coord: CoordHandle<Entity>,
    recv: RecvHandle<Entity>,
    send: SendHandle<Entity>,
) -> (CoordHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>) {
    // Drain the message queue first; if empty, skip the WS rebuild.
    let host_component_events: Vec<HostSyncEvent> = world
        .get_resource_mut::<Messages<HostSyncEvent>>()
        .map(|mut reader| reader.drain().collect())
        .unwrap_or_default();
    if host_component_events.is_empty() {
        return (coord, recv, send);
    }

    let (coord, recv, send, ()) =
        run_with_world_server(coord, recv, send, |ws| {
            // Skip drain entirely while not listening — matches today's
            // `world_to_host_sync` guard on `server.is_listening()`.
            if !ws.is_listening() {
                return;
            }
            for event in host_component_events {
                match event {
                    HostSyncEvent::Insert(_host_id, entity, component_kind) => {
                        if ws.entity_authority_status(&entity)
                            == Some(EntityAuthStatus::Denied)
                        {
                            // Client holds auth — skip (client driver
                            // will apply the insert via the receive
                            // path).
                            continue;
                        }
                        let mut world_proxy = world.proxy_mut();
                        let Some(mut component_mut) =
                            world_proxy.component_mut_of_kind(&entity, &component_kind)
                        else {
                            // Component already removed between
                            // emission and drain — same tolerant
                            // behavior as the non-pipelined path.
                            continue;
                        };
                        ws.insert_component_worldless(
                            &entity,
                            DerefMut::deref_mut(&mut component_mut),
                        );
                    }
                    HostSyncEvent::Remove(_host_id, entity, component_kind) => {
                        if ws.entity_authority_status(&entity)
                            == Some(EntityAuthStatus::Denied)
                        {
                            continue;
                        }
                        ws.remove_component_worldless(&entity, &component_kind);
                    }
                    HostSyncEvent::Despawn(_host_id, entity) => {
                        if ws.entity_authority_status(&entity)
                            == Some(EntityAuthStatus::Denied)
                        {
                            continue;
                        }
                        ws.despawn_entity_worldless(&entity);
                    }
                }
            }
        });

    (coord, recv, send)
}
