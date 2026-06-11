//! Recv-side half of a per-user `Connection` (step 4-D / 4-E.2d).
//!
//! Owns recv-thread-exclusive state: `BaseRecvConnection` (which today
//! holds only `ack_recv`), `PingManager`, `TickBufferReceiver`,
//! `timeout_timer`, `manual_disconnect`, plus a shared
//! `Arc<ConnectionShared>` cell crossing the recv/send boundary (used
//! for ACK/RTT atomic snapshots and B4 disconnect signalling).

use std::{net::SocketAddr, sync::Arc};

use naia_shared::{
    BaseRecvConnection, ChannelKinds, ConnectionConfig, StandardHeader, Tick, Timer,
};

use crate::{
    connection::{
        ping_config::PingConfig, ping_manager::PingManager,
        tick_buffer_messages::TickBufferMessages, tick_buffer_receiver::TickBufferReceiver,
    },
    server::connection_shared::ConnectionShared,
    user::UserKey,
};

/// Recv-side half of a server-side `Connection`.
pub struct RecvConnection {
    /// Remote address of the user.
    pub address: SocketAddr,
    /// User key for this connection.
    pub user_key: UserKey,
    /// Recv-side half of the base connection (ack pipeline).
    pub base: BaseRecvConnection,
    /// Pong handling / RTT estimation.
    pub ping_manager: PingManager,
    /// Buffer for tick-buffered messages decoded from inbound packets.
    pub tick_buffer: TickBufferReceiver,
    /// Set when this user has requested disconnection (manual disconnect).
    pub manual_disconnect: bool,
    /// Fires when no packet has been received within
    /// `disconnection_timeout_duration`.
    pub timeout_timer: Timer,
    /// Shared per-connection state crossing the recv/send boundary.
    pub shared: Arc<ConnectionShared>,
}

impl RecvConnection {
    /// Construct a new recv half. Takes the recv-side `BaseRecvConnection`
    /// pre-built by [`naia_shared::BaseConnection::new_split`] (so the
    /// crossbeam channel is shared with the matching `SendConnection`).
    pub fn new(
        connection_config: &ConnectionConfig,
        ping_config: &PingConfig,
        user_address: SocketAddr,
        user_key: UserKey,
        channel_kinds: &ChannelKinds,
        base: BaseRecvConnection,
        shared: Arc<ConnectionShared>,
    ) -> Self {
        Self {
            address: user_address,
            user_key,
            base,
            ping_manager: PingManager::new(ping_config),
            tick_buffer: TickBufferReceiver::new(channel_kinds),
            manual_disconnect: false,
            timeout_timer: Timer::new(connection_config.disconnection_timeout_duration),
            shared,
        }
    }

    /// True when no packet has been received within the disconnect timeout.
    pub fn should_drop(&self) -> bool {
        self.timeout_timer.ringing()
    }

    /// Record the recv-side bookkeeping for an incoming packet header: push
    /// acked-index samples onto the cross-half channel, reset the timeout,
    /// and publish the recv-derived ack snapshot onto the shared atomic
    /// cell. Composite call sites pair this with `SendConnection::drain_acks`.
    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base.ack_recv.process_incoming_header(header);
        self.timeout_timer.reset();
        let last_rx = self.base.ack_recv.last_received_packet_index();
        let bits = self.base.ack_recv.ack_bitfield();
        self.shared.set_remote_ack_seq(last_rx);
        self.shared.set_remote_ack_bitfield(bits);
    }

    /// Drain tick-buffered messages received from this user for `tick` into
    /// the shared `TickBufferMessages` accumulator (recv-only).
    pub fn tick_buffer_messages(&mut self, tick: &Tick, messages: &mut TickBufferMessages) {
        let user_key = self.user_key;
        let channel_messages = self.tick_buffer.receive_messages(tick);
        for (channel_kind, received_messages) in channel_messages {
            for message in received_messages {
                messages.push_message(&user_key, &channel_kind, message);
            }
        }
    }
}

#[cfg(feature = "test_utils")]
impl RecvConnection {
    #[doc(hidden)]
    pub fn inject_tick_buffer_message(
        &mut self,
        channel_kind: &naia_shared::ChannelKind,
        host_tick: &naia_shared::Tick,
        message_tick: &naia_shared::Tick,
        message: naia_shared::MessageContainer,
    ) -> bool {
        self.tick_buffer
            .inject_message(channel_kind, host_tick, message_tick, message)
    }
}
