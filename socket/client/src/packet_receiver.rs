use naia_socket_shared::LinkConditionerConfig;

use super::{
    conditioned_packet_receiver::ConditionedPacketReceiver, error::NaiaClientSocketError,
    server_addr::ServerAddr,
};
use crate::backends::PlainPacketReceiver;

/// Used to receive packets from the Client Socket.
///
/// A single concrete type covering both the plain and link-conditioned
/// variants: `Socket::connect*` picks one at runtime based on `SocketConfig`.
#[derive(Clone)]
pub enum PacketReceiver {
    /// Receives packets as they arrive
    Plain(PlainPacketReceiver),
    /// Receives packets through a link conditioner (latency/jitter/loss sim)
    Conditioned(ConditionedPacketReceiver),
}

impl PacketReceiver {
    /// Creates a PacketReceiver, conditioned or not depending on the config
    pub fn new(
        inner_receiver: PlainPacketReceiver,
        conditioner_config: &Option<LinkConditionerConfig>,
    ) -> Self {
        match conditioner_config {
            Some(config) => {
                PacketReceiver::Conditioned(ConditionedPacketReceiver::new(inner_receiver, config))
            }
            None => PacketReceiver::Plain(inner_receiver),
        }
    }

    /// Receives a packet from the Client Socket
    pub fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        match self {
            PacketReceiver::Plain(receiver) => receiver.receive(),
            PacketReceiver::Conditioned(receiver) => receiver.receive(),
        }
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        match self {
            PacketReceiver::Plain(receiver) => receiver.server_addr(),
            PacketReceiver::Conditioned(receiver) => receiver.server_addr(),
        }
    }
}
