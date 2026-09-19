use std::{net::SocketAddr, sync::Arc};

use smol::channel::{Sender, TrySendError};

use crate::{shutdown::ShutdownSignal, NaiaServerSocketError};

/// Used to send packets to the Server Socket
#[derive(Clone)]
pub struct PacketSender {
    channel_sender: Sender<(SocketAddr, Box<[u8]>)>,
    // Shared shutdown trigger for the listen that created this handle: the
    // last handle drop ends the background tasks (naia-lib/naia#92).
    // Retained, never read.
    #[allow(dead_code)]
    shutdown: Option<Arc<ShutdownSignal>>,
}

impl PacketSender {
    /// Creates a new PacketSender
    pub fn new(channel_sender: Sender<(SocketAddr, Box<[u8]>)>) -> Self {
        PacketSender {
            channel_sender,
            shutdown: None,
        }
    }

    /// Attaches the shutdown trigger of the listen that created this handle.
    pub(crate) fn with_shutdown(mut self, shutdown: &Arc<ShutdownSignal>) -> Self {
        self.shutdown = Some(shutdown.clone());
        self
    }

    /// Sends a packet to the Server Socket
    pub fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), NaiaServerSocketError> {
        self.channel_sender
            .try_send((*address, payload.into()))
            .map_err(|err| match err {
                TrySendError::Full(_) => unreachable!("the channel is expected to be unbound"),
                TrySendError::Closed(_) => NaiaServerSocketError::SendError(*address),
            })
    }
}
