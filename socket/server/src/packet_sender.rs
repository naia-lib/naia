use std::net::SocketAddr;

use smol::channel::{Sender, TrySendError};

use crate::NaiaServerSocketError;

// Trait
pub trait PacketSender: PacketSenderClone + Send + Sync {
    /// Sends a packet to the Server Socket
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), NaiaServerSocketError>;
}

// Impl
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
}

impl PacketSender for PacketSenderImpl {
    /// Sends a packet to the Server Socket
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), NaiaServerSocketError> {
        self.channel_sender
            .try_send((*address, payload.into()))
            .map_err(|err| match err {
                TrySendError::Full(_) => unreachable!("the channel is expected to be unbound"),
                TrySendError::Closed(_) => NaiaServerSocketError::SendError(*address),
            })
    }
}

/// Used to clone Box<dyn PacketSender>
pub trait PacketSenderClone {
    /// Clone the boxed PacketSender
    fn clone_box(&self) -> Box<dyn PacketSender>;
}

impl<T: 'static + PacketSender + Clone> PacketSenderClone for T {
    fn clone_box(&self) -> Box<dyn PacketSender> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn PacketSender> {
    fn clone(&self) -> Box<dyn PacketSender> {
        PacketSenderClone::clone_box(self.as_ref())
    }
}
