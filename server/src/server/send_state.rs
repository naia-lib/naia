//! Send-thread state owned by the pipeline coordinator (step 4-E).
//!
//! After `WorldServer::into_pipeline_states()` consumes the WorldServer,
//! `SendState<E>` carries every field the send thread needs:
//! `send_user_connections` (the send halves of every connection), the
//! per-user priority layer, the outbound `PacketSender`, and a clone of
//! `Arc<ServerShared<E>>` for lock-free access to init-only config and
//! per-connection cross-thread atomics.
//!
//! `SendHandle<E>` owns a `SendState<E>` directly (not via `Arc<Mutex>`)
//! so the send thread runs without contending with the recv or
//! coordinator threads for connection state.

use std::{collections::HashMap, hash::Hash, net::SocketAddr, sync::Arc};

use naia_shared::UserPriorityState;

use crate::{
    connection::{io::SendIo, SendConnection},
    server::ServerShared,
    user::UserKey,
};

/// Send-thread-exclusive state lifted out of `WorldServer` (step 4-E).
pub struct SendState<E: Copy + Eq + Hash + Send + Sync> {
    /// Per-address map of send-side connection halves. Each holds a clone
    /// of the matching connection's `Arc<ConnectionShared>` for ack/RTT
    /// reads.
    pub send_user_connections: HashMap<SocketAddr, SendConnection>,

    /// Per-user priority layer. Entries evicted on scope exit; whole
    /// map entry dropped on disconnect (handled by the coordinator).
    pub user_priorities: HashMap<UserKey, UserPriorityState<E>>,

    /// Send half of the transport (step 4-E.2a). Carries the encoder,
    /// outgoing bandwidth monitor, and per-tick byte counter alongside
    /// the `Box<dyn PacketSender>`. Owned here so the send thread has
    /// exclusive mutable access without locking.
    pub send_io: SendIo,

    /// Shared init-only config + cross-thread atomic cells.
    pub shared: Arc<ServerShared<E>>,
}

// SAFETY: PacketSender is a trait object; concrete impls used by naia
// (UDP and local transports) are Send. The HashMap fields are owned
// outright. UserPriorityState contains POD numeric state.
unsafe impl<E: Copy + Eq + Hash + Send + Sync> Send for SendState<E> {}
