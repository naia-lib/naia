//! Convenience helpers consumed by cyberlith's Recv SubApp inside its
//! per-tick update schedule.
//!
//! Two pieces:
//! - [`RecvLifecycleEvent`] + [`drain_lifecycle`] — translate the
//!   connect/disconnect/error variants from a [`ReceiveOutput`]'s
//!   [`WorldEvents`] into a single plain-data enum the Recv SubApp can
//!   route to its `PendingSimControl` / `PendingSendControl` buffers
//!   without needing to import every individual event type.
//! - [`drain_tick_buffer`] — fold every recv connection's pending
//!   tick-buffered messages for a given tick into a single
//!   [`TickBufferMessages`] accumulator, ready to feed into a
//!   [`crate::pipeline_actors::TickMessageRouter`].
//!
//! These mirror what `InternalWorldServer::receive_with_world` does inline in
//! the legacy serial path (`apply_receive_output` + tick-buffer-drain),
//! but factored so the Recv SubApp can call them without holding a
//! `InternalWorldServer` or a `&mut World`.
//!
//! # Scope note
//!
//! Auth-phase events (`AuthEvent`) live on [`crate::MainEvents`] —
//! a separate channel from [`WorldEvents`] — and are NOT surfaced by
//! [`drain_lifecycle`]. Cyberlith's Recv SubApp reads them directly
//! from the auth receiver / `RecvHandle` machinery during the
//! handshake-acceptance phase, not from this helper. This helper only
//! covers post-handshake lifecycle (Connect / Disconnect / RecvError).

use std::{hash::Hash, net::SocketAddr};

use naia_shared::{DisconnectReason, Tick};

use crate::{
    connection::tick_buffer_messages::TickBufferMessages,
    events::ConnectEvent,
    events::{DisconnectEvent, ErrorEvent},
    server::receive_output::ReceiveOutput,
    user::UserKey,
    NaiaServerError, RecvHandle,
};

/// Connection-lifecycle event surfaced by [`drain_lifecycle`] from a
/// [`ReceiveOutput`]'s [`crate::WorldEvents`].
///
/// Variant coverage mirrors the three [`crate::WorldEvent`] impls that
/// [`WorldEvents`] exposes for connection lifecycle: [`ConnectEvent`],
/// [`DisconnectEvent`], [`ErrorEvent`].
#[derive(Debug)]
pub enum RecvLifecycleEvent {
    /// Handshake completed — `user_key` is now valid in the
    /// `CoordHandle`'s user store. Maps to [`ConnectEvent`].
    Connected {
        /// The newly-allocated user key.
        user_key: UserKey,
    },
    /// User disconnected (clean exit or timeout). Maps to
    /// [`DisconnectEvent`].
    Disconnected {
        /// The disconnected user's key.
        user_key: UserKey,
        /// The user's address at the time of disconnect.
        address: SocketAddr,
        /// Why the user disconnected.
        reason: DisconnectReason,
    },
    /// Recv-side error (decode / protocol). Maps to [`ErrorEvent`].
    RecvError {
        /// The reported error.
        error: NaiaServerError,
    },
}

/// Drain connection-lifecycle entries from `output.world_events` into a
/// `Vec<RecvLifecycleEvent>`.
///
/// After this call the three lifecycle event lists on
/// `output.world_events` (`connections`, `disconnections`, `errors`)
/// are empty — they have been moved into the returned `Vec`. Other
/// fields on `WorldEvents` (spawns, despawns, inserts, updates, etc.)
/// are untouched.
pub fn drain_lifecycle<E>(output: &mut ReceiveOutput<E>) -> Vec<RecvLifecycleEvent>
where
    E: Copy + Eq + Hash + Send + Sync,
{
    let mut events = Vec::new();

    for user_key in output.world_events.read::<ConnectEvent>() {
        events.push(RecvLifecycleEvent::Connected { user_key });
    }
    for (user_key, address, reason) in output.world_events.read::<DisconnectEvent>() {
        events.push(RecvLifecycleEvent::Disconnected {
            user_key,
            address,
            reason,
        });
    }
    for error in output.world_events.read::<ErrorEvent>() {
        events.push(RecvLifecycleEvent::RecvError { error });
    }

    events
}

/// Drain every recv connection's tick-buffered messages for `tick` into
/// a single [`TickBufferMessages`] accumulator.
///
/// Iterates `recv_handle.state.recv_user_connections` once, calling
/// [`crate::connection::RecvConnection::tick_buffer_messages`] per
/// connection. The result is ready to feed into a
/// [`crate::pipeline_actors::TickMessageRouter::route`] call.
pub fn drain_tick_buffer<E>(recv_handle: &mut RecvHandle<E>, tick: Tick) -> TickBufferMessages
where
    E: Copy + Eq + Hash + Send + Sync,
{
    let mut messages = TickBufferMessages::default();
    for (_addr, recv_conn) in recv_handle.state.recv_user_connections.iter_mut() {
        recv_conn.tick_buffer_messages(&tick, &mut messages);
    }
    messages
}
