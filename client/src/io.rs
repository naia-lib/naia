use naia_client_socket::{NaiaClientSocketError, Packet, PacketReceiver, PacketSender, ServerAddr};
pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, ProtocolKindType,
    Protocolize, ReplicateSafe, SequenceIterator, SharedConfig, StandardHeader, Timer, Timestamp,
    WorldMutType, WorldRefType,
};
use std::net::SocketAddr;

pub struct Io {
    packet_sender: Option<PacketSender>,
    packet_receiver: Option<PacketReceiver>,
}

impl Io {
    pub fn new() -> Self {
        Io {
            packet_sender: None,
            packet_receiver: None,
        }
    }

    pub fn load(&mut self, packet_sender: PacketSender, packet_receiver: PacketReceiver) {
        if self.packet_sender.is_some() {
            panic!("Packet sender/receiver already loaded! Cannot do this twice!");
        }

        self.packet_sender = Some(packet_sender);
        self.packet_receiver = Some(packet_receiver);
    }

    pub fn is_loaded(&self) -> bool {
        self.packet_sender.is_some()
    }

    pub fn send_packet(&mut self, packet: Packet) {
        self.packet_sender
            .as_mut()
            .expect("Cannot call Client.send_packet() until you call Client.connect()!")
            .send(packet);
    }

    pub fn receive_packet(&mut self) -> Result<Option<Packet>, NaiaClientSocketError> {
        return self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Client.receive_packet() until you call Client.connect()!")
            .receive();
    }

    pub fn server_addr(&self) -> ServerAddr {
        return self
            .packet_sender
            .as_ref()
            .expect("Cannot call Client.server_addr() until you call Client.connect()!")
            .server_addr();
    }

    pub fn server_addr_unwrapped(&self) -> SocketAddr {
        if let ServerAddr::Found(server_addr) = self
            .packet_sender
            .as_ref()
            .expect("Cannot call Client.server_addr_unwrapped() until you call Client.connect()!")
            .server_addr()
        {
            return server_addr;
        } else {
            panic!("Connection has not yet been established! Call server_addr() instead when unsure about the connection status.")
        }
    }
}
