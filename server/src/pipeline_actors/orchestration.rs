//! Cross-half orchestration helpers — Phase B.7 of MISSION_SIM_OWNS_WORLD.
//!
//! Pipeline-mode handles ([`CoordHandle`], [`RecvHandle`], [`SendHandle`])
//! each own one slice of what `WorldServer` formerly owned monolithically.
//! Several user-facing operations on `WorldServer` (e.g. `entity_take_authority`,
//! `room_mut.add_user`, `send_message`, `broadcast_message`) read or
//! mutate state that crosses multiple halves. Implementing them as direct
//! handle methods would require either (1) duplicating ~1.5k LOC of
//! `WorldServer` body or (2) introducing a `WorldServerBorrow<'a>`
//! lifetime-bound shim that duplicates the same method surface against
//! `&mut` borrows.
//!
//! Instead, [`run_with_world_server`] takes the three handles by value,
//! reassembles them into a `WorldServer` via [`WorldServer::from_pipeline_states`],
//! invokes the caller's closure against the reassembled server, and
//! re-splits via [`WorldServer::into_pipeline_states`]. The pattern
//! preserves single-source-of-truth for every WorldServer method body
//! and lets cyberlith systems call the existing `WorldServer` API verbatim
//! during the B.7 transitional phase.
//!
//! [`apply_recv_to_world`] is the pipeline-mode entry point that mirrors
//! the (otherwise-internal) `WorldServer::receive_with_world` semantics:
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
    WorldServer,
};

use super::handles::CoordHandle;

/// Re-assemble the three pipeline handles into a `WorldServer<E>`, invoke
/// `f` against it, then split back into handles. Used by cyberlith B.7
/// systems for cross-half `WorldServer` operations (room mutation, scope
/// writes that need world-entity → global-entity resolution,
/// `send_message`, `broadcast_message`, `entity_take_authority`).
///
/// Cost: one `WorldServer` struct construction + destruction per call.
/// Each is a move of the three substates + an `Arc::clone` of `shared`;
/// no allocation, no per-field copy of the inner maps.
pub fn run_with_world_server<E, R>(
    coord: CoordHandle<E>,
    recv: RecvHandle<E>,
    send: SendHandle<E>,
    f: impl FnOnce(&mut WorldServer<E>) -> R,
) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>, R)
where
    E: Copy + Eq + Hash + Send + Sync,
{
    let CoordHandle { state: coord_state, shared: _coord_shared } = coord;
    let mut ws = WorldServer::from_pipeline_states(coord_state, recv.state, send.state);
    let result = f(&mut ws);
    let (coord_state, recv_state, send_state) = ws.into_pipeline_states();
    let shared = Arc::clone(&recv_state.shared);
    (
        CoordHandle { state: coord_state, shared },
        RecvHandle { state: recv_state },
        SendHandle { state: send_state },
        result,
    )
}

/// Re-split a `WorldServer<E>` back into the three handles. Used by
/// callers that needed `&mut WorldServer` access alongside `&mut World`
/// (e.g. `configure_entity_replication`) and so couldn't use the
/// closure form of `run_with_world_server` — they construct the
/// WorldServer manually via `WorldServer::from_pipeline_states` and
/// must re-package the result via this function (since the
/// `Arc<ServerShared>` is `pub(crate)` to outside callers, they can't
/// rebuild `CoordHandle` manually).
pub fn split_world_server<E>(
    ws: WorldServer<E>,
) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>)
where
    E: Copy + Eq + Hash + Send + Sync,
{
    let (coord_state, recv_state, send_state) = ws.into_pipeline_states();
    let shared = Arc::clone(&recv_state.shared);
    (
        CoordHandle { state: coord_state, shared },
        RecvHandle { state: recv_state },
        SendHandle { state: send_state },
    )
}

/// Pipeline-mode equivalent of `WorldServer::receive_with_world` — closes
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
    coord: CoordHandle<E>,
    recv: RecvHandle<E>,
    send: SendHandle<E>,
    world: W,
    output: &mut ReceiveOutput<E>,
    server_tick: Tick,
) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>)
where
    E: Copy + Eq + Hash + Send + Sync,
    W: WorldMutType<E>,
{
    let CoordHandle { state: coord_state, shared: _coord_shared } = coord;
    let mut ws = WorldServer::from_pipeline_states(coord_state, recv.state, send.state);

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

    // Cross-half decode step: the recv path skipped this because it
    // had no SendHandle. We replicate the second half of
    // `WorldServer::receive_all_packets` here. The drain of
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
        CoordHandle { state: coord_state, shared },
        RecvHandle { state: recv_state },
        SendHandle { state: send_state },
    )
}
