use std::{
    collections::HashMap,
    io::{Read, Write, ErrorKind},
    net::{TcpStream, SocketAddr, UdpSocket, TcpListener},
    sync::{Arc, Mutex},
};

use naia_shared::{IdentityToken, LinkConditionerConfig};

use crate::user::UserAuthAddr;
use super::{
    AuthReceiver as TransportAuthReceiver, AuthSender as TransportAuthSender,
    conditioner::ConditionedPacketReceiver, PacketReceiver,
    PacketSender as TransportSender, RecvError, SendError, Socket as TransportSocket,
};

// Socket
pub struct Socket {
    data_socket: Arc<Mutex<UdpSocket>>,
    auth_io: Arc<Mutex<AuthIo>>,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(
        auth_addr: &SocketAddr,
        data_addr: &SocketAddr,
        config: Option<LinkConditionerConfig>
    ) -> Self {

        let auth_socket = TcpListener::bind(*auth_addr).unwrap();
        auth_socket
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");
        let auth_io = Arc::new(Mutex::new(AuthIo::new(auth_socket)));

        let data_socket = Arc::new(Mutex::new(UdpSocket::bind(*data_addr).unwrap()));
        data_socket
            .as_ref()
            .lock()
            .unwrap()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        Self { data_socket, auth_io, config }
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn listen(
        self: Box<Self>
    ) -> (
        Box<dyn TransportAuthSender>,
        Box<dyn TransportAuthReceiver>,
        Box<dyn TransportSender>,
        Box<dyn PacketReceiver>,
    ) {
        let auth_sender = AuthSender::new(self.auth_io.clone());
        let auth_receiver = AuthReceiver::new(self.auth_io.clone());
        let packet_sender = UdpPacketSender::new(self.data_socket.clone());
        let packet_receiver = UdpPacketReceiver::new(self.data_socket.clone());

        let packet_receiver: Box<dyn PacketReceiver> = {
            if let Some(config) = &self.config {
                Box::new(ConditionedPacketReceiver::new(packet_receiver, config))
            } else {
                Box::new(packet_receiver)
            }
        };

        return (
            Box::new(auth_sender),
            Box::new(auth_receiver),
            Box::new(packet_sender),
            packet_receiver,
        );
    }
}

// Packet Sender

struct UdpPacketSender {
    socket: Arc<Mutex<UdpSocket>>,
}

impl UdpPacketSender {
    pub fn new(socket: Arc<Mutex<UdpSocket>>) -> Self {
        return Self { socket };
    }
}

impl TransportSender for UdpPacketSender {
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
pub(crate) struct UdpPacketReceiver {
    socket: Arc<Mutex<UdpSocket>>,
    buffer: [u8; 1472],
}

impl UdpPacketReceiver {
    pub fn new(socket: Arc<Mutex<UdpSocket>>) -> Self {
        return Self {
            socket,
            buffer: [0; 1472],
        };
    }
}

impl PacketReceiver for UdpPacketReceiver {
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
                    ErrorKind::WouldBlock => Ok(None),
                    _ => Err(RecvError),
                }
            }
        }
    }
}

// AuthIo
pub(crate) struct AuthIo {
    socket: TcpListener,
    buffer: [u8; 1472],
    outgoing_streams: HashMap<SocketAddr, TcpStream>,
}

impl AuthIo {
    pub fn new(socket: TcpListener) -> Self {
        Self {
            socket,
            buffer: [0; 1472],
            outgoing_streams: HashMap::new(),
        }
    }

    fn receive(&mut self) -> Result<Option<(UserAuthAddr, &[u8])>, RecvError> {
        match self.socket.accept() {
            Ok((mut stream, addr)) => {
                let recv_len = stream.read(&mut self.buffer).unwrap();
                if self.outgoing_streams.contains_key(&addr) {
                    // already have a stream for this address
                    // TODO: handle this case?
                    return Err(RecvError);
                }
                self.outgoing_streams.insert(addr, stream);
                Ok(Some((UserAuthAddr::new(addr), &self.buffer[..recv_len])))
            }
            Err(ref e) => {
                let kind = e.kind();
                match kind {
                    ErrorKind::WouldBlock => {
                        Ok(None)
                    }
                    _ => {
                        Err(RecvError)
                    }
                }
            }
        }
    }

    /// Sends an accept packet from the Client Socket
    fn accept(
        &mut self,
        address: &UserAuthAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        if let Some(mut stream) = self.outgoing_streams.remove(&address.addr()) {
            let success_byte: u8 = 1;
            stream.write(&[success_byte]).unwrap();
            let id_token_bytes = identity_token.as_bytes();
            stream.write(id_token_bytes).unwrap();
            stream.flush().unwrap();
            return Ok(());
        }
        Err(SendError)
    }

    /// Sends a rejection packet from the Client Socket
    fn reject(&mut self, address: &UserAuthAddr) -> Result<(), SendError> {
        if let Some(mut stream) = self.outgoing_streams.remove(&address.addr()) {
            let fail_byte: u8 = 0;
            stream.write(&[fail_byte]).unwrap();
            stream.flush().unwrap();
            return Ok(());
        }
        Err(SendError)
    }
}

// AuthSender
#[derive(Clone)]
pub(crate) struct AuthSender {
    auth_io: Arc<Mutex<AuthIo>>,
}

impl AuthSender {
    pub fn new(auth_io: Arc<Mutex<AuthIo>>) -> Self {
        Self { auth_io }
    }
}

impl TransportAuthSender for AuthSender {
    /// Sends an accept packet from the Client Socket
    fn accept(
        &self,
        address: &UserAuthAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        self.auth_io.lock().unwrap().accept(address, identity_token)
    }

    /// Sends a rejection packet from the Client Socket
    fn reject(&self, address: &UserAuthAddr) -> Result<(), SendError> {
        self.auth_io.lock().unwrap().reject(address)
    }
}

// AuthReceiver
#[derive(Clone)]
pub(crate) struct AuthReceiver {
    auth_io: Arc<Mutex<AuthIo>>,
    buffer: Box<[u8]>
}

impl AuthReceiver {
    pub fn new(auth_io: Arc<Mutex<AuthIo>>) -> Self {
        Self { auth_io, buffer: Box::new([0; 1472]) }
    }
}

impl TransportAuthReceiver for AuthReceiver {
    fn receive(&mut self) -> Result<Option<(UserAuthAddr, &[u8])>, RecvError> {
        let mut guard = self.auth_io.lock().unwrap();
        match guard.receive() {
            Ok(option) => match option {
                Some((addr, buffer)) => {
                    self.buffer = buffer.into();
                    Ok(Some((addr, &self.buffer)))
                }
                None => Ok(None),
            },
            Err(err) => Err(err),
        }
    }
}