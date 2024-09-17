use std::time::Duration;

use log::warn;
use naia_client_socket::shared::IdentityToken;

use naia_shared::{
    handshake::HandshakeHeader, BitReader, BitWriter, OutgoingPacket, PacketType, Serde,
    StandardHeader, Timer, Timestamp as stamp_time,
};

use crate::{
    connection::time_manager::TimeManager,
    handshake::{handshake_time_manager::HandshakeTimeManager, HandshakeResult, Handshaker},
};

type Timestamp = u64;

enum HandshakeState {
    AwaitingChallengeResponse,
    AwaitingValidateResponse,
    TimeSync(HandshakeTimeManager),
    AwaitingConnectResponse(TimeManager),
    Connected,
}

impl HandshakeState {
    fn get_index(&self) -> u8 {
        match self {
            HandshakeState::AwaitingChallengeResponse => 0,
            HandshakeState::AwaitingValidateResponse => 1,
            HandshakeState::TimeSync(_) => 2,
            HandshakeState::AwaitingConnectResponse(_) => 3,
            HandshakeState::Connected => 4,
        }
    }
}

impl Eq for HandshakeState {}

impl PartialEq for HandshakeState {
    fn eq(&self, other: &Self) -> bool {
        other.get_index() == self.get_index()
    }
}

pub struct HandshakeManager {
    ping_interval: Duration,
    handshake_pings: u8,
    connection_state: HandshakeState,
    handshake_timer: Timer,
    identity_token: Option<IdentityToken>,
    pre_connection_timestamp: Timestamp,
    pre_connection_digest: Option<Vec<u8>>,
}

impl Handshaker for HandshakeManager {
    fn set_identity_token(&mut self, identity_token: IdentityToken) {
        self.identity_token = Some(identity_token);
    }

    // fn is_connected(&self) -> bool {
    //     self.connection_state == HandshakeState::Connected
    // }

    // Give handshake manager the opportunity to send out messages to the server
    fn send(&mut self) -> Option<OutgoingPacket> {
        if !self.handshake_timer.ringing() {
            return None;
        }

        self.handshake_timer.reset();

        match &mut self.connection_state {
            HandshakeState::AwaitingChallengeResponse => {
                if let Some(identity_token) = &self.identity_token {
                    let writer = self.write_challenge_request(identity_token);
                    return Some(writer.to_packet());
                } else {
                    // warn!("Identity Token not set");
                    return None;
                }
            }
            HandshakeState::AwaitingValidateResponse => {
                let writer = self.write_validate_request();
                return Some(writer.to_packet());
            }
            HandshakeState::TimeSync(time_manager) => {
                // use time manager to send initial pings until client/server time is synced
                // then, move state to AwaitingConnectResponse
                let writer = time_manager.write_ping();
                return Some(writer.to_packet());
            }
            HandshakeState::AwaitingConnectResponse(_) => {
                let writer = self.write_connect_request();
                return Some(writer.to_packet());
            }
            HandshakeState::Connected => {
                // do nothing, not necessary
                return None;
            }
        }
    }

    // Call this regularly so handshake manager can process incoming requests
    fn recv(&mut self, reader: &mut BitReader) -> Option<HandshakeResult> {
        let header_result = StandardHeader::de(reader);
        if header_result.is_err() {
            return None;
        }
        let header = header_result.unwrap();
        match header.packet_type {
            PacketType::Handshake => {
                let Ok(handshake_header) = HandshakeHeader::de(reader) else {
                    warn!("Could not read HandshakeHeader");
                    return None;
                };
                match handshake_header {
                    HandshakeHeader::ServerChallengeResponse => {
                        self.recv_challenge_response(reader);
                        return None;
                    }
                    HandshakeHeader::ServerValidateResponse => {
                        if self.connection_state == HandshakeState::AwaitingValidateResponse {
                            self.recv_validate_response();
                        }
                        return None;
                    }
                    HandshakeHeader::ServerConnectResponse => {
                        return self.recv_connect_response();
                    }
                    HandshakeHeader::ClientChallengeRequest
                    | HandshakeHeader::ClientValidateRequest
                    | HandshakeHeader::ClientConnectRequest
                    | HandshakeHeader::Disconnect => {
                        return None;
                    }
                }
            }
            PacketType::Pong => {
                // Time Manager should record incoming Pongs in order to sync time
                let mut success = false;
                if let HandshakeState::TimeSync(time_manager) = &mut self.connection_state {
                    let Ok(success_inner) = time_manager.read_pong(reader) else {
                        // TODO: bubble this up
                        warn!("Time Manager cannot process pong");
                        return None;
                    };
                    success = success_inner;
                }
                if success {
                    let HandshakeState::TimeSync(time_manager) =
                        std::mem::replace(&mut self.connection_state, HandshakeState::Connected)
                    else {
                        panic!("should be impossible due to check above");
                    };
                    self.connection_state =
                        HandshakeState::AwaitingConnectResponse(time_manager.finalize());
                }
                return None;
            }
            PacketType::Data | PacketType::Heartbeat | PacketType::Ping => {
                return None;
            }
        }
    }

    // Write a disconnect packet
    fn write_disconnect(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::Disconnect.ser(&mut writer);
        self.write_signed_timestamp(&mut writer);
        writer
    }
}

impl HandshakeManager {
    pub fn new(send_interval: Duration, ping_interval: Duration, handshake_pings: u8) -> Self {
        let mut handshake_timer = Timer::new(send_interval);
        handshake_timer.ring_manual();

        let pre_connection_timestamp = stamp_time::now();

        Self {
            handshake_timer,
            identity_token: None,
            pre_connection_timestamp,
            pre_connection_digest: None,
            connection_state: HandshakeState::AwaitingChallengeResponse,
            ping_interval,
            handshake_pings,
        }
    }

    // Step 1 of Handshake
    fn write_challenge_request(&self, identity_token: &IdentityToken) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ClientChallengeRequest.ser(&mut writer);

        self.pre_connection_timestamp.ser(&mut writer);
        identity_token.ser(&mut writer);

        writer
    }

    // Step 2 of Handshake
    fn recv_challenge_response(&mut self, reader: &mut BitReader) {
        if self.connection_state == HandshakeState::AwaitingChallengeResponse {
            let timestamp_result = Timestamp::de(reader);
            if timestamp_result.is_err() {
                return;
            }
            let timestamp = timestamp_result.unwrap();

            if self.pre_connection_timestamp == timestamp {
                let digest_bytes_result = Vec::<u8>::de(reader);
                if digest_bytes_result.is_err() {
                    return;
                }
                let digest_bytes = digest_bytes_result.unwrap();
                self.pre_connection_digest = Some(digest_bytes);

                self.connection_state = HandshakeState::AwaitingValidateResponse;
            }
        }
    }

    // Step 3 of Handshake
    fn write_validate_request(&self) -> BitWriter {
        let mut writer = BitWriter::new();

        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ClientValidateRequest.ser(&mut writer);

        // write timestamp & digest into payload
        self.write_signed_timestamp(&mut writer);

        writer
    }

    // Step 4 of Handshake
    fn recv_validate_response(&mut self) {
        self.connection_state = HandshakeState::TimeSync(HandshakeTimeManager::new(
            self.ping_interval,
            self.handshake_pings,
        ));
    }

    // Step 5 of Handshake
    fn write_connect_request(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ClientConnectRequest.ser(&mut writer);

        writer
    }

    // Step 6 of Handshake
    fn recv_connect_response(&mut self) -> Option<HandshakeResult> {
        let HandshakeState::AwaitingConnectResponse(time_manager) =
            std::mem::replace(&mut self.connection_state, HandshakeState::Connected)
        else {
            return None;
        };

        return Some(HandshakeResult::Connected(time_manager));
    }

    fn write_signed_timestamp(&self, writer: &mut BitWriter) {
        self.pre_connection_timestamp.ser(writer);
        let digest: &Vec<u8> = self.pre_connection_digest.as_ref().unwrap();
        digest.ser(writer);
    }
}
