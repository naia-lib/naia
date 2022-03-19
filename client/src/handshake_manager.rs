use std::time::Duration;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_client_socket::Packet;

pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, ProtocolKindType,
    ProtocolType, ReplicateSafe, SequenceIterator, SharedConfig, StandardHeader, Timer, Timestamp,
    WorldMutType, WorldRefType,
};

use super::{
    connection_state::{ConnectionState, ConnectionState::AwaitingChallengeResponse},
    io::Io,
    tick_manager::TickManager,
};

#[derive(PartialEq)]
pub enum HandshakeResult {
    None,
    Connected,
}

pub struct HandshakeManager<P: ProtocolType> {
    handshake_timer: Timer,
    pre_connection_timestamp: Option<Timestamp>,
    pre_connection_digest: Option<Box<[u8]>>,
    connection_state: ConnectionState,
    auth_message: Option<P>,
}

impl<P: ProtocolType> HandshakeManager<P> {
    pub fn new(send_interval: Duration) -> Self {
        let mut handshake_timer = Timer::new(send_interval);
        handshake_timer.ring_manual();

        Self {
            handshake_timer,
            pre_connection_timestamp: None,
            pre_connection_digest: None,
            connection_state: AwaitingChallengeResponse,
            auth_message: None,
        }
    }

    pub fn send_packet(&mut self, io: &mut Io) {
        if !self.handshake_timer.ringing() {
            return;
        }

        self.handshake_timer.reset();

        match self.connection_state {
            ConnectionState::Connected => {
                // do nothing, not necessary
            }
            ConnectionState::AwaitingChallengeResponse => {
                if self.pre_connection_timestamp.is_none() {
                    self.pre_connection_timestamp = Some(Timestamp::now());
                }

                let mut timestamp_bytes = Vec::new();
                self.pre_connection_timestamp
                    .as_mut()
                    .unwrap()
                    .write(&mut timestamp_bytes);
                internal_send_connectionless(
                    io,
                    PacketType::ClientChallengeRequest,
                    Packet::new(timestamp_bytes),
                );
            }
            ConnectionState::AwaitingConnectResponse => {
                // write timestamp & digest into payload
                let mut payload_bytes = Vec::new();
                self.pre_connection_timestamp
                    .as_mut()
                    .unwrap()
                    .write(&mut payload_bytes);
                for digest_byte in self.pre_connection_digest.as_ref().unwrap().as_ref() {
                    payload_bytes.push(*digest_byte);
                }
                // write auth message if there is one
                if let Some(auth_message) = &mut self.auth_message {
                    let auth_dyn = auth_message.dyn_ref();
                    let auth_kind = auth_dyn.get_kind();
                    // write that we have auth
                    payload_bytes.write_u8(1).unwrap();
                    // write auth kind
                    payload_bytes
                        .write_u16::<BigEndian>(auth_kind.to_u16())
                        .unwrap();
                    // write payload
                    auth_dyn.write(&mut payload_bytes);
                } else {
                    // write that we do not have auth
                    payload_bytes.write_u8(0).unwrap();
                }
                internal_send_connectionless(
                    io,
                    PacketType::ClientConnectRequest,
                    Packet::new(payload_bytes),
                );
            }
        }
    }

    pub fn set_auth_message(&mut self, auth: P) {
        self.auth_message = Some(auth);
    }

    pub fn disconnect(&mut self) {
        self.pre_connection_timestamp = None;
        self.pre_connection_digest = None;
        self.connection_state = AwaitingChallengeResponse;
    }

    pub fn receive_packet(
        &mut self,
        tick_manager: &mut Option<TickManager>,
        packet: Packet,
    ) -> HandshakeResult {
        let (header, payload) = StandardHeader::read(packet.payload());
        match header.packet_type() {
            PacketType::ServerChallengeResponse => {
                if self.connection_state == ConnectionState::AwaitingChallengeResponse {
                    if let Some(my_timestamp) = self.pre_connection_timestamp {
                        let mut reader = PacketReader::new(&payload);
                        let server_tick = reader.cursor().read_u16::<BigEndian>().unwrap();
                        let payload_timestamp = Timestamp::read(&mut reader);

                        if my_timestamp == payload_timestamp {
                            let mut digest_bytes: Vec<u8> = Vec::new();
                            for _ in 0..32 {
                                digest_bytes.push(reader.read_u8());
                            }
                            self.pre_connection_digest = Some(digest_bytes.into_boxed_slice());

                            if let Some(tick_manager) = tick_manager {
                                tick_manager.set_initial_tick(server_tick);
                            }

                            self.connection_state = ConnectionState::AwaitingConnectResponse;
                        }
                    }
                }
            }
            PacketType::ServerConnectResponse => {
                self.connection_state = ConnectionState::Connected;
                return HandshakeResult::Connected;
            }
            _ => {}
        }

        return HandshakeResult::None;
    }
}

fn internal_send_connectionless(io: &mut Io, packet_type: PacketType, packet: Packet) {
    let new_payload =
        naia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
    io.send_packet(Packet::new_raw(new_payload));
}
