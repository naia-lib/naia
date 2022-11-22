use std::net::SocketAddr;

use smol::channel::{Sender, TrySendError};

/// Used to send packets to the Server Socket
#[derive(Clone)]
pub struct PacketSender {
    channel_sender: Sender<(SocketAddr, Box<[u8]>)>,
}

impl PacketSender {
    /// Creates a new PacketSender
    pub fn new(channel_sender: Sender<(SocketAddr, Box<[u8]>)>) -> Self {
        PacketSender { channel_sender }
    }

    /// Sends a packet to the Server Socket
    pub fn send(
        &self,
        address: &SocketAddr,
        payload: &[u8],
    ) -> Result<(), naia_socket_shared::TrySendError<()>> {
        self.channel_sender
            .try_send((*address, payload.into()))
            .map_err(|err| match err {
                TrySendError::Full(_) => naia_socket_shared::TrySendError::Full(()),
                TrySendError::Closed(_) => naia_socket_shared::TrySendError::Closed(()),
            })
    }
}
