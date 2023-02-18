use log::warn;
use std::time::Duration;

use crate::connection::time_config::TimeConfig;
use crate::connection::time_manager::TimeManager;
use naia_shared::{BitReader, BitWriter, FakeEntityConverter, Message, MessageKinds, Serde};
pub use naia_shared::{
    ConnectionConfig, PacketType, Replicate, StandardHeader, Timer, Timestamp as stamp_time,
    WorldMutType, WorldRefType,
};

use super::io::Io;

pub type Timestamp = u64;

#[derive(Eq, PartialEq)]
pub enum HandshakeState {
    AwaitingChallengeResponse,
    AwaitingValidateResponse,
    TimeSync,
    AwaitingConnectResponse,
    Connected,
}

pub enum HandshakeResult {
    Connected(TimeManager),
    Rejected,
}

pub struct HandshakeManager {
    pub connection_state: HandshakeState,
    handshake_timer: Timer,
    pre_connection_timestamp: Timestamp,
    pre_connection_digest: Option<Vec<u8>>,
    auth_message: Option<Box<dyn Message>>,
    time_config: TimeConfig,
    tick_duration: Duration,
    time_manager: Option<TimeManager>,
}

impl HandshakeManager {
    pub fn new(send_interval: Duration, time_config: TimeConfig, tick_duration: Duration) -> Self {
        let mut handshake_timer = Timer::new(send_interval);
        handshake_timer.ring_manual();

        let pre_connection_timestamp = stamp_time::now();

        Self {
            handshake_timer,
            pre_connection_timestamp,
            pre_connection_digest: None,
            connection_state: HandshakeState::AwaitingChallengeResponse,
            auth_message: None,
            time_config,
            tick_duration,
            time_manager: None,
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

            match self.connection_state {
                HandshakeState::AwaitingChallengeResponse => {
                    let mut writer = self.write_challenge_request();
                    if io.send_writer(&mut writer).is_err() {
                        // TODO: pass this on and handle above
                        warn!("Client Error: Cannot send challenge request packet to Server");
                    }
                }
                HandshakeState::AwaitingValidateResponse => {
                    let mut writer = self.write_validate_request(message_kinds);
                    if io.send_writer(&mut writer).is_err() {
                        // TODO: pass this on and handle above
                        warn!("Client Error: Cannot send validate request packet to Server");
                    }
                }
                HandshakeState::TimeSync => {
                    // use time manager to send initial pings until client/server time is synced
                    // then, move state to AwaitingConnectResponse
                    let Some(time_manager) = &mut self.time_manager else {
                        panic!("Client Error: Time Manager should be initialized at this point in handshake");
                    };

                    if time_manager.handshake_finished() {
                        self.connection_state = HandshakeState::AwaitingConnectResponse;
                    } else {
                        time_manager.handshake_send(io);
                    }
                }
                HandshakeState::AwaitingConnectResponse => {
                    let mut writer = self.write_connect_request();
                    if io.send_writer(&mut writer).is_err() {
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
                self.recv_validate_response();
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
                if let Some(time_manager) = &mut self.time_manager {
                    if time_manager.process_pong(reader).is_err() {
                        // TODO: bubble this up
                        warn!("Time Manager cannot process pong");
                    }
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
        self.connection_state = HandshakeState::TimeSync;
        self.time_manager = Some(TimeManager::new(&self.time_config, &self.tick_duration));
    }

    // Step 6 of Handshake
    pub fn write_connect_request(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::ClientConnectRequest, 0, 0, 0).ser(&mut writer);

        writer
    }

    // Step 7 of Handshake
    fn recv_connect_response(&mut self) -> Option<HandshakeResult> {
        if self.connection_state == HandshakeState::Connected {
            return None;
        }

        self.connection_state = HandshakeState::Connected;

        let time_manager = self
            .time_manager
            .take()
            .expect("How could there be no Time Manager here?");

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
