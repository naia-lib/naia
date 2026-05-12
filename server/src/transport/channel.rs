use std::net::SocketAddr;

use smol::{
    channel,
    channel::{Receiver, Sender, TryRecvError},
};

use super::{
    PacketReceiver as TransportReceiver, PacketSender as TransportSender, RecvError, SendError,
};

/// In-process MPSC channel bridging a `PacketSender` / `PacketReceiver` pair for local transport.
pub struct PacketChannel;

impl PacketChannel {
    /// Creates an unbounded in-process channel and returns the sender/receiver pair.
    pub fn unbounded() -> (Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let (packet_sender, packet_receiver) = channel::unbounded();
        let packet_receiver = PacketChannelReceiver::new(packet_receiver);
        (Box::new(packet_sender), Box::new(packet_receiver))
    }
}

impl TransportSender for Sender<(SocketAddr, Box<[u8]>)> {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        self.send_blocking((*address, payload.into()))
            .map_err(|_| SendError)
    }
}

#[derive(Clone)]
struct PacketChannelReceiver {
    receiver: Receiver<(SocketAddr, Box<[u8]>)>,
    current_payload: Option<Box<[u8]>>,
}

impl PacketChannelReceiver {
    fn new(receiver: Receiver<(SocketAddr, Box<[u8]>)>) -> Self {
        Self {
            receiver,
            current_payload: None,
        }
    }
}

impl TransportReceiver for PacketChannelReceiver {
    /// Receives a packet from the Server Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        match self.receiver.try_recv() {
            Ok((address, payload)) => {
                self.current_payload = Some(payload);
                Ok(Some((address, self.current_payload.as_ref().unwrap())))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(_) => Err(RecvError),
        }
    }
}
