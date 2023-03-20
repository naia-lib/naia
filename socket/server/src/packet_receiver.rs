use std::net::SocketAddr;

use smol::channel::Receiver;

use super::error::NaiaServerSocketError;

/// Used to receive packets from the Server Socket
pub trait PacketReceiver: PacketReceiverClone + Send + Sync {
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

impl PacketReceiver for PacketReceiverImpl {
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

/// Used to clone Box<dyn PacketReceiver>
pub trait PacketReceiverClone {
    /// Clone the boxed PacketReceiver
    fn clone_box(&self) -> Box<dyn PacketReceiver>;
}

impl<T: 'static + PacketReceiver + Clone> PacketReceiverClone for T {
    fn clone_box(&self) -> Box<dyn PacketReceiver> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn PacketReceiver> {
    fn clone(&self) -> Box<dyn PacketReceiver> {
        PacketReceiverClone::clone_box(self.as_ref())
    }
}
