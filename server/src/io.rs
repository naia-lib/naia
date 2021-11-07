use std::panic;

use naia_server_socket::{NaiaServerSocketError, Packet, PacketReceiver, PacketSender};

pub use naia_shared::{
    wrapping_diff, Connection, ConnectionConfig, Instant, KeyGenerator, LocalComponentKey,
    ManagerType, Manifest, PacketReader, PacketType, PropertyMutate, PropertyMutator,
    ProtocolKindType, ProtocolType, Replicate, ReplicateSafe, SharedConfig, StandardHeader, Timer,
    Timestamp, WorldMutType, WorldRefType,
};

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

    pub fn send_packet(&self, packet: Packet) {
        self.packet_sender
            .as_ref()
            .expect("Cannot call Server.send_packet() until you call Server.listen()!")
            .send(packet);
    }

    pub fn receive_packet(&mut self) -> Result<Option<Packet>, NaiaServerSocketError> {
        return self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Server.receive_packet() until you call Server.listen()!")
            .receive();
    }
}
