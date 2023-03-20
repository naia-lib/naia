

use std::{io::ErrorKind, net::{UdpSocket, SocketAddr}, sync::{Arc, Mutex}};

use naia_shared::SocketConfig;

use super::{
    PacketReceiver as TransportReceiver, PacketSender as TransportSender, RecvError, SendError,
    Socket as TransportSocket,
};

// Socket
pub struct Socket {
    socket: Arc<Mutex<UdpSocket>>,
    config: SocketConfig,
}

impl Socket {
    pub fn new(server_addr: &SocketAddr, config: &SocketConfig) -> Self {

        let socket = Arc::new(Mutex::new(UdpSocket::bind(*server_addr).unwrap()));
        socket
            .as_ref()
            .lock()
            .unwrap()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        return Self {
            socket,
            config: config.clone(),
        };
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn listen(self: Box<Self>) -> (Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let inner_sender = PacketSender::new(self.socket.clone());
        let inner_receiver = PacketReceiver::new(self.socket.clone());
        return (Box::new(inner_sender), Box::new(inner_receiver));
    }
}

// Packet Sender
struct PacketSender {
    socket: Arc<Mutex<UdpSocket>>,
}

impl PacketSender {
    pub fn new(socket: Arc<Mutex<UdpSocket>>) -> Self {
        return Self {
            socket,
        };
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
        match self.socket.as_ref().lock().unwrap().recv_from(&mut self.buffer) {
            Ok((recv_len, address)) => {
                Ok(Some((address, &self.buffer[..recv_len])))
            }
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
