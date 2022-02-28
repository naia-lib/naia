use std::time::Duration;

pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PacketType, ProtocolKindType,
    Protocolize, ReplicateSafe, SharedConfig, StandardHeader, Timer, Timestamp, WorldMutType,
    WorldRefType,
};
use naia_shared::serde::{BitReader, BitWriter, Serde};

use super::io::Io;

#[derive(Debug, PartialEq)]
enum HandshakeState {
    AwaitingChallengeResponse,
    AwaitingConnectResponse,
    Connected,
}

pub struct HandshakeManager<P: Protocolize> {
    handshake_timer: Timer,
    pre_connection_timestamp: Option<Timestamp>,
    pre_connection_digest: Option<Box<[u8]>>,
    connection_state: HandshakeState,
    auth_message: Option<P>,
}

impl<P: Protocolize> HandshakeManager<P> {
    pub fn new(send_interval: Duration) -> Self {
        let mut handshake_timer = Timer::new(send_interval);
        handshake_timer.ring_manual();

        Self {
            handshake_timer,
            pre_connection_timestamp: None,
            pre_connection_digest: None,
            connection_state: HandshakeState::AwaitingChallengeResponse,
            auth_message: None,
        }
    }

    pub fn send_packet(&mut self, io: &mut Io) {
        if !self.handshake_timer.ringing() {
            return;
        }

        self.handshake_timer.reset();

        match self.connection_state {
            HandshakeState::Connected => {
                // do nothing, not necessary
            }
            HandshakeState::AwaitingChallengeResponse => {
                let mut writer = BitWriter::new();
                StandardHeader::new(PacketType::ClientChallengeRequest, 0, 0, 0, 0)
                    .ser(&mut writer);

                if self.pre_connection_timestamp.is_none() {
                    self.pre_connection_timestamp = Some(Timestamp::now());
                }

                self.pre_connection_timestamp
                    .as_mut()
                    .unwrap()
                    .to_u64()
                    .ser(&mut writer);

                io.send_writer(&mut writer);
            }
            HandshakeState::AwaitingConnectResponse => {
                let mut writer = BitWriter::new();

                StandardHeader::new(PacketType::ClientConnectRequest, 0, 0, 0, 0)
                    .ser(&mut writer);

                // write timestamp & digest into payload
                self.write_signed_timestamp(&mut writer);

                // write auth message if there is one
                if let Some(auth_message) = &mut self.auth_message {
                    // write that we have auth
                    1.ser(&mut writer);
                    // write auth kind
                    auth_message.dyn_ref().kind().ser(&mut writer);
                    // write payload
                    auth_message.write(&mut writer);
                } else {
                    // write that we do not have auth
                    0.ser(&mut writer);
                }

                io.send_writer(&mut writer);
            }
        }
    }

    /// Get an outgoing Disconnect payload
    pub fn write_disconnect_packet(&self, writer: &mut BitWriter) {
        self.write_signed_timestamp(writer);
    }

    pub fn set_auth_message(&mut self, auth: P) {
        self.auth_message = Some(auth);
    }

    // Returns whether connection was successful
    pub fn receive_packet(&mut self, reader: &mut BitReader) -> bool {
        let header = StandardHeader::de(reader).unwrap();
        match header.packet_type() {
            PacketType::ServerChallengeResponse => {
                if self.connection_state == HandshakeState::AwaitingChallengeResponse {
                    if let Some(my_timestamp) = self.pre_connection_timestamp {

                        let payload_timestamp = Timestamp::from_u64(&u64::de(reader).unwrap());

                        if my_timestamp == payload_timestamp {
                            let mut digest_bytes: Vec<u8> = Vec::new();
                            for _ in 0..32 {
                                digest_bytes.push(u8::de(reader).unwrap());
                            }
                            self.pre_connection_digest = Some(digest_bytes.into_boxed_slice());

                            self.connection_state = HandshakeState::AwaitingConnectResponse;
                        }
                    }
                }
            }
            PacketType::ServerConnectResponse => {
                self.connection_state = HandshakeState::Connected;
                return true;
            }
            _ => {}
        }

        return false;
    }

    fn write_signed_timestamp(&self, writer: &mut BitWriter) {
        self.pre_connection_timestamp
            .as_ref()
            .unwrap()
            .to_u64()
            .ser(writer);
        for digest_byte in self.pre_connection_digest.as_ref().unwrap().as_ref() {
            digest_byte.ser(writer);
        }
    }
}
