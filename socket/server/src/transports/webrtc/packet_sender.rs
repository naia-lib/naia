use std::net::SocketAddr;
use smol::channel::{Sender, TrySendError};
use naia_socket_shared::ChannelClosedError;
use crate::NaiaServerSocketError;
use crate::packet_sender::PacketSenderTrait;

/// Used to send packets to the Server Socket
#[derive(Clone)]
pub struct PacketSenderImpl {
    channel_sender: Sender<(SocketAddr, Box<[u8]>)>,
}

impl PacketSenderImpl {
    /// Creates a new PacketSender
    pub fn new(channel_sender: Sender<(SocketAddr, Box<[u8]>)>) -> Self {
        PacketSenderImpl { channel_sender }
    }

    /// Sends a packet to the Server Socket
    pub fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), ChannelClosedError<()>> {
        self.channel_sender
            .try_send((*address, payload.into()))
            .map_err(|err| match err {
                TrySendError::Full(_) => unreachable!("the channel is expected to be unbound"),
                TrySendError::Closed(_) => ChannelClosedError(()),
            })
    }
}

impl PacketSenderTrait for PacketSenderImpl {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), NaiaServerSocketError> {
        self.send(address, payload)
            .map_err(|err| NaiaServerSocketError::Wrapped(Box::new(err)))
    }
}