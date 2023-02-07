use std::net::SocketAddr;
use smol::channel::Receiver;
use crate::NaiaServerSocketError;
use crate::packet_receiver::PacketReceiverTrait;

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
