use std::time::Duration;

use log::warn;

use naia_shared::{
    BitReader, BitWriter, FakeEntityConverter, Message, MessageKinds, PacketType, Serde,
    StandardHeader, Timer, Timestamp as stamp_time,
};

use super::io::Io;
use crate::connection::{handshake_time_manager::HandshakeTimeManager, time_manager::TimeManager};

pub type Timestamp = u64;

pub enum HandshakeState {
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

pub enum HandshakeResult {
    Connected(TimeManager),
    Rejected,
}

pub struct HandshakeManager {
    ping_interval: Duration,
    handshake_pings: u8,
    pub connection_state: HandshakeState,
    handshake_timer: Timer,
    pre_connection_timestamp: Timestamp,
    pre_connection_digest: Option<Vec<u8>>,
    auth_message: Option<Box<dyn Message>>,
}

impl HandshakeManager {
    pub fn new(send_interval: Duration, ping_interval: Duration, handshake_pings: u8) -> Self {
        let mut handshake_timer = Timer::new(send_interval);
        handshake_timer.ring_manual();

        let pre_connection_timestamp = stamp_time::now();

        Self {
            handshake_timer,
            pre_connection_timestamp,
            pre_connection_digest: None,
            connection_state: HandshakeState::AwaitingChallengeResponse,
            auth_message: None,
            ping_interval,
            handshake_pings,
        }
    }

    pub fn set_auth_message(&mut self, auth: Box<dyn Message>) {
        self.auth_message = Some(auth);
    }

    pub fn is_connected(&self) -> bool {
        self.connection_state == HandshakeState::Connected
    }

    // Give handshake manager the opportunity to send out messages to the server
    pub fn send(&mut self, message_kinds: &MessageKinds, io: &mut Io) {
        if io.is_loaded() {
            if !self.handshake_timer.ringing() {
                return;
            }

            self.handshake_timer.reset();

            match &mut self.connection_state {
                HandshakeState::AwaitingChallengeResponse => {
                    let writer = self.write_challenge_request();
                    if io.send_packet(writer.to_packet()).is_err() {
                        // TODO: pass this on and handle above
                        warn!("Client Error: Cannot send challenge request packet to Server");
                    }
                }
                HandshakeState::AwaitingValidateResponse => {
                    let writer = self.write_validate_request(message_kinds);
                    if io.send_packet(writer.to_packet()).is_err() {
                        // TODO: pass this on and handle above
                        warn!("Client Error: Cannot send validate request packet to Server");
                    }
                }
                HandshakeState::TimeSync(time_manager) => {
                    // use time manager to send initial pings until client/server time is synced
                    // then, move state to AwaitingConnectResponse
                    time_manager.send_ping(io);
                }
                HandshakeState::AwaitingConnectResponse(_) => {
                    let writer = self.write_connect_request();
                    if io.send_packet(writer.to_packet()).is_err() {
                        // TODO: pass this on and handle above
                        warn!("Client Error: Cannot send connect request packet to Server");
                    }
                }
                HandshakeState::Connected => {
                    // do nothing, not necessary
                }
            }
        }
    }

    // Call this regularly so handshake manager can process incoming requests
    pub fn recv(&mut self, reader: &mut BitReader) -> Option<HandshakeResult> {
        let header_result = StandardHeader::de(reader);
        if header_result.is_err() {
            return None;
        }
        let header = header_result.unwrap();
        match header.packet_type {
            PacketType::ServerChallengeResponse => {
                self.recv_challenge_response(reader);
                return None;
            }
            PacketType::ServerValidateResponse => {
                if self.connection_state == HandshakeState::AwaitingValidateResponse {
                    self.recv_validate_response();
                }
                return None;
            }
            PacketType::ServerConnectResponse => {
                return self.recv_connect_response();
            }
            PacketType::ServerRejectResponse => {
                return Some(HandshakeResult::Rejected);
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
                    let HandshakeState::TimeSync(time_manager) = std::mem::replace(&mut self.connection_state, HandshakeState::Connected) else {
                        panic!("should be impossible due to check above");
                    };
                    self.connection_state =
                        HandshakeState::AwaitingConnectResponse(time_manager.finalize());
                }
                return None;
            }
            PacketType::Data
            | PacketType::Heartbeat
            | PacketType::ClientChallengeRequest
            | PacketType::ClientValidateRequest
            | PacketType::ClientConnectRequest
            | PacketType::Ping
            | PacketType::Disconnect => {
                return None;
            }
        }
    }

    // Step 1 of Handshake
    pub fn write_challenge_request(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::ClientChallengeRequest, 0, 0, 0).ser(&mut writer);

        self.pre_connection_timestamp.ser(&mut writer);

        writer
    }

    // Step 2 of Handshake
    pub fn recv_challenge_response(&mut self, reader: &mut BitReader) {
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
    pub fn write_validate_request(&self, message_kinds: &MessageKinds) -> BitWriter {
        let mut writer = BitWriter::new();

        StandardHeader::new(PacketType::ClientValidateRequest, 0, 0, 0).ser(&mut writer);

        // write timestamp & digest into payload
        self.write_signed_timestamp(&mut writer);

        // write auth message if there is one
        if let Some(auth_message) = &self.auth_message {
            // write that we have auth
            true.ser(&mut writer);
            // write payload
            auth_message.write(message_kinds, &mut writer, &FakeEntityConverter);
        } else {
            // write that we do not have auth
            false.ser(&mut writer);
        }

        writer
    }

    // Step 4 of Handshake
    pub fn recv_validate_response(&mut self) {
        self.connection_state = HandshakeState::TimeSync(HandshakeTimeManager::new(
            self.ping_interval,
            self.handshake_pings,
        ));
    }

    // Step 5 of Handshake
    pub fn write_connect_request(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::ClientConnectRequest, 0, 0, 0).ser(&mut writer);

        writer
    }

    // Step 6 of Handshake
    fn recv_connect_response(&mut self) -> Option<HandshakeResult> {
        let HandshakeState::AwaitingConnectResponse(time_manager) = std::mem::replace(&mut self.connection_state, HandshakeState::Connected) else {
            return None;
        };

        return Some(HandshakeResult::Connected(time_manager));
    }

    // Send 10 disconnect packets
    pub fn write_disconnect(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Disconnect, 0, 0, 0).ser(&mut writer);
        self.write_signed_timestamp(&mut writer);
        writer
    }

    // Private methods

    fn write_signed_timestamp(&self, writer: &mut BitWriter) {
        self.pre_connection_timestamp.ser(writer);
        let digest: &Vec<u8> = self.pre_connection_digest.as_ref().unwrap();
        digest.ser(writer);
    }
}
