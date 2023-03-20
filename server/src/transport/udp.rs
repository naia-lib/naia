use std::{
    io::ErrorKind,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use naia_shared::LinkConditionerConfig;

use super::{
    conditioner::ConditionedPacketReceiver, PacketReceiver as TransportReceiver,
    PacketSender as TransportSender, RecvError, SendError, Socket as TransportSocket,
};

// Socket
pub struct Socket {
    socket: Arc<Mutex<UdpSocket>>,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(server_addr: &SocketAddr, config: Option<LinkConditionerConfig>) -> Self {
        let socket = Arc::new(Mutex::new(UdpSocket::bind(*server_addr).unwrap()));
        socket
            .as_ref()
            .lock()
            .unwrap()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        return Self { socket, config };
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn listen(self: Box<Self>) -> (Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let sender = Box::new(PacketSender::new(self.socket.clone()));

        let receiver: Box<dyn TransportReceiver> = {
            let inner_receiver = Box::new(PacketReceiver::new(self.socket.clone()));
            if let Some(config) = &self.config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        return (sender, receiver);
    }
}

// Packet Sender
struct PacketSender {
    socket: Arc<Mutex<UdpSocket>>,
}

impl PacketSender {
    pub fn new(socket: Arc<Mutex<UdpSocket>>) -> Self {
        return Self { socket };
    }
}

impl TransportSender for PacketSender {
    /// Sends a packet from the Client Socket
    fn send(&self, socket_addr: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        if self
            .socket
            .as_ref()
            .lock()
            .unwrap()
            .send_to(payload, *socket_addr)
            .is_err()
        {
            return Err(SendError);
        }
        return Ok(());
    }
}

// Packet Receiver
#[derive(Clone)]
struct PacketReceiver {
    socket: Arc<Mutex<UdpSocket>>,
    buffer: [u8; 1472],
}

impl PacketReceiver {
    pub fn new(socket: Arc<Mutex<UdpSocket>>) -> Self {
        return Self {
            socket,
            buffer: [0; 1472],
        };
    }
}

impl TransportReceiver for PacketReceiver {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        match self
            .socket
            .as_ref()
            .lock()
            .unwrap()
            .recv_from(&mut self.buffer)
        {
            Ok((recv_len, address)) => Ok(Some((address, &self.buffer[..recv_len]))),
            Err(ref e) => {
                let kind = e.kind();
                match kind {
                    ErrorKind::WouldBlock => {
                        return Ok(None);
                    }
                    _ => {
                        return Err(RecvError);
                    }
                }
            }
        }
    }
}
