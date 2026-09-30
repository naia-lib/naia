use std::{net::SocketAddr, sync::Arc};

use smol::channel::Receiver;

use naia_socket_shared::LinkConditionerConfig;

use super::{
    conditioned_packet_receiver::ConditionedPacketReceiver, error::NaiaServerSocketError,
    shutdown::ShutdownSignal,
};

/// Used to receive packets from the Server Socket.
///
/// A single concrete type covering both the plain and link-conditioned
/// variants: `Socket::listen*` picks one at runtime based on `SocketConfig`.
#[derive(Clone)]
pub enum PacketReceiver {
    /// Receives packets as they arrive
    Plain(PlainPacketReceiver),
    /// Receives packets through a link conditioner (latency/jitter/loss sim)
    Conditioned(ConditionedPacketReceiver),
}

impl PacketReceiver {
    /// Receives a packet from the Server Socket
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        match self {
            PacketReceiver::Plain(receiver) => receiver.receive(),
            PacketReceiver::Conditioned(receiver) => receiver.receive(),
        }
    }

    /// Creates a PacketReceiver, conditioned or not depending on the config
    #[allow(clippy::type_complexity)]
    pub fn new(
        channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
        conditioner_config: &Option<LinkConditionerConfig>,
    ) -> Self {
        match conditioner_config {
            Some(config) => PacketReceiver::Conditioned(ConditionedPacketReceiver::new(
                channel_receiver,
                config,
            )),
            None => PacketReceiver::Plain(PlainPacketReceiver::new(channel_receiver)),
        }
    }

    /// Attaches the shutdown trigger of the listen that created this handle.
    pub(crate) fn with_shutdown(mut self, shutdown: &Arc<ShutdownSignal>) -> Self {
        match &mut self {
            PacketReceiver::Plain(receiver) => {
                receiver.shutdown = Some(shutdown.clone());
            }
            PacketReceiver::Conditioned(receiver) => {
                receiver.shutdown = Some(shutdown.clone());
            }
        }
        self
    }
}

/// Used to receive packets from the Server Socket
#[derive(Clone)]
pub struct PlainPacketReceiver {
    #[allow(clippy::type_complexity)]
    channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    last_payload: Option<Box<[u8]>>,
    // Shared shutdown trigger for the listen that created this handle: the
    // last handle drop ends the background tasks (naia-lib/naia#92).
    // Retained, never read.
    #[allow(dead_code)]
    shutdown: Option<Arc<ShutdownSignal>>,
}

impl PlainPacketReceiver {
    /// Creates a new PlainPacketReceiver
    #[allow(clippy::type_complexity)]
    pub fn new(
        channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    ) -> Self {
        Self {
            channel_receiver,
            last_payload: None,
            shutdown: None,
        }
    }

    /// Receives a packet from the Server Socket
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        match self.channel_receiver.try_recv() {
            Ok(result) => match result {
                Ok((address, payload)) => {
                    self.last_payload = Some(payload);
                    Ok(Some((address, self.last_payload.as_ref().unwrap())))
                }
                Err(_) => Ok(None),
            },
            Err(_) => Ok(None),
        }
    }
}
