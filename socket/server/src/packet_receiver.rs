use std::net::SocketAddr;

use crossbeam::channel::Receiver;

use super::error::NaiaServerSocketError;

/// Used to receive packets from the Server Socket
#[derive(Clone)]
pub struct PacketReceiver {
    inner: Box<dyn PacketReceiverTrait>,
}

impl PacketReceiver {
    /// Create a new PacketReceiver
    pub fn new(inner: Box<dyn PacketReceiverTrait>) -> Self {
        PacketReceiver { inner }
    }

    /// Receives a packet from the Server Socket
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        self.inner.receive()
    }
}

/// Used to receive packets from the Server Socket
pub trait PacketReceiverTrait: PacketReceiverClone + Send + Sync {
    /// Receives a packet from the Server Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError>;
}

/// Used to receive packets from the Server Socket
#[derive(Clone)]
pub struct PacketReceiverImpl {
    #[allow(clippy::type_complexity)]
    channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    last_payload: Option<Box<[u8]>>,
}

impl PacketReceiverImpl {
    /// Creates a new PacketReceiver
    #[allow(clippy::type_complexity)]
    pub fn new(
        channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    ) -> Self {
        PacketReceiverImpl {
            channel_receiver,
            last_payload: None,
        }
    }
}

impl PacketReceiverTrait for PacketReceiverImpl {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        match self.channel_receiver.try_recv() {
            Ok(result) => match result {
                Ok((address, payload)) => {
                    self.last_payload = Some(payload);
                    return Ok(Some((address, self.last_payload.as_ref().unwrap())));
                }
                Err(_) => Ok(None),
            },
            Err(_) => Ok(None),
        }
    }
}

/// Used to clone Box<dyn PacketReceiverTrait>
pub trait PacketReceiverClone {
    /// Clone the boxed PacketReceiver
    fn clone_box(&self) -> Box<dyn PacketReceiverTrait>;
}

impl<T: 'static + PacketReceiverTrait + Clone> PacketReceiverClone for T {
    fn clone_box(&self) -> Box<dyn PacketReceiverTrait> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn PacketReceiverTrait> {
    fn clone(&self) -> Box<dyn PacketReceiverTrait> {
        PacketReceiverClone::clone_box(self.as_ref())
    }
}
