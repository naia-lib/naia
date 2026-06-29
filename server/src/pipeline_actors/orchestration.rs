//! Cross-half orchestration helpers — Phase B.7 of MISSION_SIM_OWNS_WORLD.
//!
//! Pipeline-mode handles ([`CoordHandle`], [`RecvHandle`], [`SendHandle`])
//! each own one slice of what `InternalWorldServer` formerly owned monolithically.
//! Several user-facing operations on `InternalWorldServer` (e.g. `entity_take_authority`,
//! `room_mut.add_user`, `send_message`, `broadcast_message`) read or
//! mutate state that crosses multiple halves. Implementing them as direct
//! handle methods would require either (1) duplicating ~1.5k LOC of
//! `InternalWorldServer` body or (2) introducing a `WorldServerBorrow<'a>`
//! lifetime-bound shim that duplicates the same method surface against
//! `&mut` borrows.
//!
//! Instead, [`run_with_world_server`] takes the three handles by value,
//! reassembles them into a `InternalWorldServer` via [`InternalWorldServer::from_pipeline_states`],
//! invokes the caller's closure against the reassembled server, and
//! re-splits via [`InternalWorldServer::into_pipeline_states`]. The pattern
//! preserves single-source-of-truth for every InternalWorldServer method body
//! and lets cyberlith systems call the existing `InternalWorldServer` API verbatim
//! during the B.7 transitional phase.
//!
//! [`apply_recv_to_world`] is the pipeline-mode entry point that mirrors
//! the (otherwise-internal) `InternalWorldServer::receive_with_world` semantics:
//! it consumes a [`ReceiveOutput`] populated by [`RecvHandle::receive`],
//! drives the cross-half `process_recv_packets` + world-mutation half
//! of `process_all_packets`, accumulates the resulting world events
//! back into the same [`ReceiveOutput`], and returns the three handles.
//! Closes the gap that `RecvHandle::receive` documents at
//! `pipeline_handles.rs:84` (where the recv path skips
//! `process_all_packets` because it has no `&mut World`).

use std::hash::Hash;
use std::sync::Arc;

use naia_shared::{Instant, Tick, WorldMutType};

use crate::server::{
    pipeline_handles::{RecvHandle, SendHandle},
    receive_output::ReceiveOutput,
    InternalWorldServer,
};

use super::handles::CoordHandle;

/// Re-assemble the three pipeline handles into a `InternalWorldServer<E>`, invoke
/// `f` against it, then split back into handles. Used by cyberlith B.7
/// systems for cross-half `InternalWorldServer` operations (room mutation, scope
/// writes that need world-entity → global-entity resolution,
/// `send_message`, `broadcast_message`, `entity_take_authority`).
///
/// Cost: one `InternalWorldServer` struct construction + destruction per call.
/// Each is a move of the three substates + an `Arc::clone` of `shared`;
/// no allocation, no per-field copy of the inner maps.
pub fn run_with_world_server<E, R>(
    sim_handle: CoordHandle<E>,
    recv: RecvHandle<E>,
    send: SendHandle<E>,
    f: impl FnOnce(&mut InternalWorldServer<E>) -> R,
) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>, R)
where
    E: Copy + Eq + Hash + Send + Sync,
{
    let CoordHandle {
        state: coord_state,
        shared: _coord_shared,
    } = sim_handle;
    let mut ws = InternalWorldServer::from_pipeline_states(coord_state, recv.state, send.state);
    let result = f(&mut ws);
    let (coord_state, recv_state, send_state) = ws.into_pipeline_states();
    let shared = Arc::clone(&recv_state.shared);
    (
        CoordHandle {
            state: coord_state,
            shared,
        },
        RecvHandle { state: recv_state },
        SendHandle { state: send_state },
        result,
    )
}

/// MISSION_USER_ONLY_SEES_SIM Phase B.2 (2026-05-19) —
/// pipeline-friendly entry point for `configure_entity_replication`.
///
/// Cyberlith's pre-B.2 wrapper (`server_access::configure_entity_
/// replication_with_world`) inlined the take/call/put dance against
/// three `Option<*Handle>` Bevy Resources and a `WorldProxyMut` borrow.
/// This function folds the whole pattern into one naia call so that
/// post-Phase-C the cyberlith Sim system shrinks to roughly:
///
/// ```ignore
/// fn sim_system(world: &mut World) {
///     let sim_handle = world.resource_mut::<NaiaSimResource>().0.take().unwrap();
///     let recv  = world.resource_mut::<NaiaRecvResource>().0.take().unwrap();
///     let send  = world.resource_mut::<SendHandleResource>().0.take().unwrap();
///     let (sim_handle, recv, send) = configure_entity_replication(
///         sim_handle, recv, send,
///         &mut WorldProxyMut::proxy_mut(world),
///         &entity, config,
///     );
///     world.resource_mut::<NaiaSimResource>().0 = Some(sim_handle);
///     world.resource_mut::<NaiaRecvResource>().0 = Some(recv);
///     world.resource_mut::<SendHandleResource>().0 = Some(send);
/// }
/// ```
///
/// As of Phase D.2.4 this delegates to the Coord-only
/// [`CoordHandle::configure_entity_replication`] + eager deferred-op
/// drains (see the D.2.4 section below). Wire output remains
/// byte-identical to the legacy `InternalWorldServer::configure_entity_replication`
/// path — the B.2 test `sim_configure_entity_replication.rs` still
/// enforces this against this exact facade, unchanged.
///
/// # Historical: why B.2 originally reassembled `InternalWorldServer`
///
/// B.2 (commit `1e9cf38f`) shipped this facade as a thin wrapper around
/// [`run_with_world_server`] that called `InternalWorldServer::configure_entity_
/// replication` verbatim. The spec (`MISSION_USER_ONLY_SEES_SIM.md`
/// §6.2) had proposed a true `CoordHandle::configure_entity_replication`
/// with the Send-side acknowledgment deferred to
/// `apply_pending_send_preamble` via a new `ScopeChange::ConfigureReplication`
/// variant, but an end-to-end audit of `InternalWorldServer::configure_entity_
/// replication` (`world_server.rs:1423`) found three blockers that
/// deferred that design to Phase D.2 (each resolved there — see the
/// D.2.4 section):
///
/// 1. The Send-side state machine is read-before-write across multiple
///    locks (e.g. `unpublish_entity` captures `owner_addr` from sim_handle
///    BEFORE `gwm.entity_unpublish` transitions ClientPublic → Client).
///    Splitting capture from mutation across tick boundaries requires
///    snapshotting half a dozen pieces of state into the variant
///    payload — substantial state-machine surface area to maintain in
///    two places.
///
/// 2. The `enable_delegation_client_owned_entity` sub-path
///    (`world_server.rs:2719`) does protocol-ordered work on
///    `send_user_connections`: `migrate_entity_remote_to_host` +
///    `host_local_enable_delegation` + `host_send_migrate_response`
///    must run synchronously so that `MigrateResponse` is the FIRST
///    message in the new HostEntityChannel sequence (subcommand_id=0).
///    Deferring breaks the per-channel sequencing contract.
///
/// 3. `world.entity_publish` / `world.entity_unpublish` /
///    `world.entity_enable_delegation` install per-component diff
///    mutators on the Bevy world (`adapters/bevy/shared/src/component
///    _access.rs:207`). In Sim-owns-world this is the Sim world. The
///    preamble has no world parameter today; adding one breaks every
///    existing caller. Storing world-mutation work in a separate
///    queue drained by a NEW `apply_pending_world_hooks<W>` method
///    splits the state machine across two methods and forces every
///    consumer to remember to call both.
///
/// # Phase D.2.4 (2026-05-19) — delegates to the Coord-only API
///
/// As of D.2.4 this facade NO LONGER reassembles a `InternalWorldServer` via
/// [`run_with_world_server`]. The three D.2 blockers quoted above were
/// resolved by D.2.1–D.2.3:
///
/// - **Blocker 1** (read-before-write across half-boundaries) — fixed
///   by capturing the pre-transition gwm/user-store state into the
///   `ScopeChange::ConfigureReplication` payload at queue-push time
///   (see [`CoordHandle::configure_entity_replication`] and
///   `configure_replication::ConfigureCapture`).
/// - **Blocker 2** (`MigrateResponse`-as-first-message ordering in the
///   client-origin delegation path) — fixed by D.2.3's explicit
///   `HostEntityChannel::reserve_first_command` invariant, so the
///   migration sequence can run inside the deferred Send drain.
/// - **Blocker 3** (world component-hook registration) — fixed by the
///   separate `pending_world_hooks` queue drained by
///   [`CoordHandle::apply_pending_world_hooks`] (or its `SendHandle`
///   twin), which the holder of `&mut World` calls explicitly.
///
/// This facade now performs the full operation EAGERLY (synchronous
/// contract): it must leave Coord + Send + World in the same state the
/// legacy `run_with_world_server` path did, by the time it returns. So
/// it drains the deferred Send leaf-ops and the deferred World hooks
/// before returning rather than waiting for a future tick:
///
/// 1. `sim_handle.configure_entity_replication(entity, config)` — runs the
///    publicity decision tree once against pre-transition gwm state,
///    applies the Coord-side gwm writes immediately, and queues the
///    Send leaf-ops (`ScopeChange::ConfigureReplication`) plus the
///    World hooks (`pending_world_hooks`).
/// 2. `send.state.apply_pending_configure_replication(...)` — the
///    NARROW Send drain. It removes ONLY `ScopeChange::ConfigureReplication`
///    variants from `scope_change_queue` and applies their Send leaf-ops;
///    every other variant (`EntityEnteredRoom`, `ScopeToggled`,
///    `RoomChange`, …) is left untouched in queue order. This matches
///    the legacy facade exactly: `InternalWorldServer::configure_entity_replication`
///    applied the Send work inline WITHOUT running the per-tick send
///    preamble (no heartbeats / empty-acks / outbound flush / no
///    `preamble_done_this_tick` flag). Calling the broader
///    [`SendHandle::apply_pending_send_preamble`] here would introduce
///    those extra per-tick side effects and so would NOT be
///    byte-identical — hence the narrow entry point.
/// 3. `sim_handle.apply_pending_world_hooks(world)` — drains the deferred
///    World component-hook ops onto `world`, installing the per-component
///    diff mutators the legacy `world.entity_publish` / `entity_unpublish`
///    / `entity_enable_delegation` calls would have installed inline.
///
/// `recv` is now unused by the body (the operation never touched
/// Recv-side state) but is kept in the signature, by-position and
/// by-type, so cyberlith's existing call site at HEAD keeps compiling.
/// Phase E may simplify the signature later, atomically with the
/// cyberlith call-site update.
///
/// Byte-identity vs the legacy path is enforced unchanged by the B.2
/// test `sim_configure_entity_replication.rs` (it still calls this
/// facade) plus the D.2.2 deferred-drain parity tests.
pub fn configure_entity_replication<E, W>(
    sim_handle: CoordHandle<E>,
    recv: RecvHandle<E>,
    mut send: SendHandle<E>,
    world: &mut W,
    entity: &E,
    config: crate::world::replication_config::ReplicationConfig,
) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>)
where
    E: Copy + Eq + Hash + Send + Sync + 'static,
    W: WorldMutType<E>,
{
    let mut sim_handle = sim_handle;

    // 1. Coord-side gwm writes immediately; queue deferred Send + World ops.
    sim_handle.configure_entity_replication(entity, config);

    // 2. Narrow Send drain — apply ONLY the just-queued ConfigureReplication
    //    leaf-ops, leaving every other scope-change variant in place. No
    //    preamble side effects (matches the legacy inline behavior).
    let shared = Arc::clone(&send.state.shared);
    send.state
        .apply_pending_configure_replication(&shared.scope_change_queue);

    // 3. World-side component-hook drain onto the caller's world.
    sim_handle.apply_pending_world_hooks(world);

    // `recv` carries no state for this op; preserved in the signature for
    // cyberlith-build-stability (see doc-comment).
    let _ = &recv;

    (sim_handle, recv, send)
}

/// Re-split a `InternalWorldServer<E>` back into the three handles. Used by
/// callers that needed `&mut InternalWorldServer` access alongside `&mut World`
/// (e.g. `configure_entity_replication`) and so couldn't use the
/// closure form of `run_with_world_server` — they construct the
/// InternalWorldServer manually via `InternalWorldServer::from_pipeline_states` and
/// must re-package the result via this function (since the
/// `Arc<ServerShared>` is `pub(crate)` to outside callers, they can't
/// rebuild `CoordHandle` manually).
pub fn split_world_server<E>(ws: InternalWorldServer<E>) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>)
where
    E: Copy + Eq + Hash + Send + Sync,
{
    let (coord_state, recv_state, send_state) = ws.into_pipeline_states();
    let shared = Arc::clone(&recv_state.shared);
    (
        CoordHandle {
            state: coord_state,
            shared,
        },
        RecvHandle { state: recv_state },
        SendHandle { state: send_state },
    )
}

/// Pipeline-mode equivalent of `InternalWorldServer::receive_with_world` — closes
/// the world-mutation gap that [`RecvHandle::receive`] leaves open.
///
/// `output` is the per-tick `ReceiveOutput<E>` previously produced by
/// `recv.receive()`. After this call, `output.world_events` contains
/// the complete set of world events that the serial-mode
/// `receive_with_world` would have produced — including the
/// Spawn / Insert / Update / Despawn / queued-disconnect entries that
/// `RecvHandle::receive` could not generate on its own (because
/// `process_all_packets` requires `&mut World`).
///
/// The three handles are consumed and returned re-split. Caller pattern:
/// `world.remove_resource` → call → `world.insert_resource` of each.
///
/// `server_tick` is the current server tick that the cross-half
/// `SendHandle::process_recv_packets` decode step needs to evaluate
/// tick-buffered messages against.
pub fn apply_recv_to_world<E, W>(
    sim_handle: CoordHandle<E>,
    recv: RecvHandle<E>,
    send: SendHandle<E>,
    // MISSION_PIPELINE_API_BOUNDARY G8b: `&mut W` (not by value) so
    // `PipelinedServer::receive` can reborrow one world across the several
    // `ReceiveOutput`s a worker-shape channel burst yields per tick.
    world: &mut W,
    output: &mut ReceiveOutput<E>,
    server_tick: Tick,
) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>)
where
    E: Copy + Eq + Hash + Send + Sync,
    W: WorldMutType<E>,
{
    let CoordHandle {
        state: coord_state,
        shared: _coord_shared,
    } = sim_handle;
    let mut ws = InternalWorldServer::from_pipeline_states(coord_state, recv.state, send.state);

    // Pre-stuff the already-collected handshake-time world events from
    // `RecvHandle::receive` into the reassembled server's recv-side
    // incoming buffer, so the subsequent `take_world_events` at the
    // tail returns the combined set in the original order
    // (handshake events first, then the data-packet-derived events
    // that process_all_packets appends).
    let prior_world_events = std::mem::replace(
        &mut ws.recv.incoming_world_events,
        crate::events::WorldEvents::<E>::new(),
    );
    ws.recv.incoming_world_events = std::mem::replace(&mut output.world_events, prior_world_events);
    // After the swap: output.world_events holds an empty fresh
    // WorldEvents (target of the merged drain below); ws.recv.incoming_world_events
    // holds the prior events ready to be augmented by process_all_packets.

    // Drain pending handshakes — in pipeline mode (RecvState::receive),
    // handshake packets are DEFERRED to `shared.pending_handshakes`
    // instead of calling `finalize_connection` inline. The coordination stage
    // (us) finalizes them, which fires ConnectEvent into
    // `recv.incoming_world_events` AND inserts new SendConnections
    // into `send.send_user_connections` (via
    // `commit_pending_send_state_updates`). Without this, the new
    // connection never appears send-side and any `send_message` /
    // `broadcast_message` to it silently drops.
    ws.drain_pending_handshakes();

    // Cross-half decode step: the recv path skipped this because it
    // had no SendHandle. We replicate the second half of
    // `InternalWorldServer::receive_all_packets` here. The drain of
    // `received_addresses` + `pending_data_packets` mirrors the
    // pipeline-mode coordinator step from
    // `SendHandle::process_recv_packets`'s doc-comment.
    let received_addresses = std::mem::take(&mut output.received_addresses);
    let pending_data_packets = std::mem::take(&mut output.pending_data_packets);
    ws.send.process_recv_packets(
        &mut ws.recv.recv_user_connections,
        received_addresses,
        pending_data_packets,
        server_tick,
    );

    // Apply the decoded entity events to `world` and accumulate
    // Spawn/Despawn/Insert/Update entries into recv.incoming_world_events.
    let now = Instant::now();
    ws.process_all_packets(world, &now);

    // Drain the combined event set back into output.world_events.
    output.world_events = ws.take_world_events();

    let (coord_state, recv_state, send_state) = ws.into_pipeline_states();
    let shared = Arc::clone(&recv_state.shared);
    (
        CoordHandle {
            state: coord_state,
            shared,
        },
        RecvHandle { state: recv_state },
        SendHandle { state: send_state },
    )
}
