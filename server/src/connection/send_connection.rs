//! Send-side half of a per-user `Connection` (step 4-D).
//!
//! Owns send-thread-exclusive state: `BaseSendConnection` (message
//! manager, world manager, ack send, heartbeat timer, bandwidth
//! accumulator), `visibility` bitset, and a shared
//! `Arc<ConnectionShared>` cell crossing the recv/send boundary.

use std::{net::SocketAddr, sync::Arc};

use naia_shared::{
    BaseSendConnection, ConnectionVisibilityBitset, GlobalEntityIndex, PacketNotifiable,
};

use crate::{server::connection_shared::ConnectionShared, user::UserKey};

/// Send-side half of a server-side `Connection`.
pub struct SendConnection {
    /// Remote address of the user.
    pub address: SocketAddr,
    /// User key for this connection.
    pub user_key: UserKey,
    /// Send-side half of the base connection.
    pub base: BaseSendConnection,
    /// Per-connection entity visibility bitset. One bit per `GlobalEntityIndex`.
    /// Set when an entity enters scope; cleared on despawn or pause.
    pub visibility: ConnectionVisibilityBitset,
    /// Shared per-connection state crossing the recv/send boundary.
    pub shared: Arc<ConnectionShared>,
}

impl SendConnection {
    /// Construct a new send half. Takes the send-side `BaseSendConnection`
    /// pre-built by [`naia_shared::BaseConnection::new_split`] (so the
    /// crossbeam channel is shared with the matching `RecvConnection`).
    pub fn new(
        user_address: SocketAddr,
        user_key: UserKey,
        base: BaseSendConnection,
        max_replicated_entities: usize,
        shared: Arc<ConnectionShared>,
    ) -> Self {
        Self {
            address: user_address,
            user_key,
            base,
            // capacity = max_replicated_entities + 1 (slot 0 = INVALID sentinel)
            visibility: ConnectionVisibilityBitset::new(max_replicated_entities + 1),
            shared,
        }
    }

    /// Set entity `idx` as visible for this connection (scope entry or resume).
    pub fn set_entity_visible(&mut self, idx: GlobalEntityIndex) {
        self.visibility.set(idx);
    }

    /// Clear entity `idx` as not visible for this connection (scope exit or pause).
    pub fn clear_entity_visible(&mut self, idx: GlobalEntityIndex) {
        self.visibility.clear(idx);
    }

    /// Drain pending acked-index samples from the cross-half channel,
    /// removing acknowledged entries from `sent_packets`, updating the
    /// loss monitor, and firing delivery notifications on the message
    /// manager and world manager. Invoked from
    /// `Connection::process_incoming_header` in step 4-D for behavioral
    /// parity; once the coordinator wires the recv/send split, the send
    /// thread calls this at the top of its own send cycle.
    pub fn drain_acks(&mut self, extras: &mut [&mut dyn PacketNotifiable]) {
        let naia_shared::BaseSendConnection {
            message_manager,
            world_manager,
            ack_send,
            ..
        } = &mut self.base;
        let mut base_notifiables: [&mut dyn PacketNotifiable; 2] =
            [message_manager, world_manager];
        ack_send.drain_samples(&mut base_notifiables, extras);
    }
}
