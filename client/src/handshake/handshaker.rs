use std::time::Duration;

use log::warn;

#[cfg(feature = "transport_udp")]
use naia_shared::Timestamp as stamp_time;
use naia_shared::{
    handshake::HandshakeHeader, BitReader, BitWriter, IdentityToken, OutgoingPacket, PacketType,
    ProtocolId, Serde, StandardHeader, Timer,
};

use crate::{
    connection::time_manager::TimeManager,
    handshake::{handshake_time_manager::HandshakeTimeManager, HandshakeResult, Handshaker},
};

#[cfg(feature = "transport_udp")]
type Timestamp = u64;

/// The client's side of the session negotiation.
///
/// Both builds run the same negotiation: present the identity token so the
/// server can bind this socket address to an already-authenticated user, sync
/// clocks, then request the connection. Builds with source-address validation
/// (raw UDP, where a spoofed source address is cheap) prepend an HMAC
/// challenge/validate round-trip; builds without it (WebRTC, where ICE/DTLS
/// already proves address ownership) start at identify instead.
enum HandshakeState {
    #[cfg(feature = "transport_udp")]
    AwaitingChallengeResponse,
    #[cfg(feature = "transport_udp")]
    AwaitingValidateResponse,
    #[cfg(not(feature = "transport_udp"))]
    AwaitingIdentifyResponse,
    TimeSync(HandshakeTimeManager),
    AwaitingConnectResponse(TimeManager),
    Connected,
}

impl HandshakeState {
    fn get_index(&self) -> u8 {
        match self {
            #[cfg(feature = "transport_udp")]
            HandshakeState::AwaitingChallengeResponse => 0,
            #[cfg(feature = "transport_udp")]
            HandshakeState::AwaitingValidateResponse => 1,
            #[cfg(not(feature = "transport_udp"))]
            HandshakeState::AwaitingIdentifyResponse => 0,
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
    protocol_id: ProtocolId,
    ping_interval: Duration,
    handshake_pings: u8,
    connection_state: HandshakeState,
    handshake_timer: Timer,
    identity_token: Option<IdentityToken>,
    #[cfg(feature = "transport_udp")]
    pre_connection_timestamp: Timestamp,
    #[cfg(feature = "transport_udp")]
    pre_connection_digest: Option<Vec<u8>>,
}

impl Handshaker for HandshakeManager {
    fn set_identity_token(&mut self, identity_token: IdentityToken) {
        self.identity_token = Some(identity_token);
    }

    // Give handshake manager the opportunity to send out messages to the server
    fn send(&mut self) -> Option<OutgoingPacket> {
        if !self.handshake_timer.ringing() {
            return None;
        }

        self.handshake_timer.reset();

        match &mut self.connection_state {
            #[cfg(feature = "transport_udp")]
            HandshakeState::AwaitingChallengeResponse => {
                let identity_token = self.identity_token.as_ref()?;
                let writer = self.write_challenge_request(identity_token);
                Some(writer.to_packet())
            }
            #[cfg(feature = "transport_udp")]
            HandshakeState::AwaitingValidateResponse => {
                let writer = self.write_validate_request();
                Some(writer.to_packet())
            }
            #[cfg(not(feature = "transport_udp"))]
            HandshakeState::AwaitingIdentifyResponse => {
                if let Some(identity_token) = &self.identity_token {
                    let writer = self.write_identify_request(identity_token);
                    Some(writer.to_packet())
                } else {
                    warn!("HandshakeManager: Timer ringing but Identity Token not set");
                    None
                }
            }
            HandshakeState::TimeSync(time_manager) => {
                let writer = time_manager.write_ping();
                Some(writer.to_packet())
            }
            HandshakeState::AwaitingConnectResponse(_) => {
                let writer = self.write_connect_request();
                Some(writer.to_packet())
            }
            HandshakeState::Connected => None,
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
                    #[cfg(feature = "transport_udp")]
                    HandshakeHeader::ServerChallengeResponse => {
                        self.recv_challenge_response(reader);
                        None
                    }
                    #[cfg(feature = "transport_udp")]
                    HandshakeHeader::ServerValidateResponse => {
                        if self.connection_state == HandshakeState::AwaitingValidateResponse {
                            self.recv_validate_response();
                        }
                        None
                    }
                    #[cfg(not(feature = "transport_udp"))]
                    HandshakeHeader::ServerIdentifyResponse => {
                        self.recv_identify_response(reader);
                        None
                    }
                    HandshakeHeader::ServerConnectResponse => self.recv_connect_response(),
                    HandshakeHeader::ServerRejectResponse(reason) => {
                        Some(HandshakeResult::Rejected(reason))
                    }
                    _ => None,
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
                None
            }
            PacketType::Data | PacketType::Heartbeat | PacketType::Ping => None,
        }
    }

    // Write a disconnect packet
    #[cfg(feature = "transport_udp")]
    fn write_disconnect(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::Disconnect.ser(&mut writer);
        self.write_signed_timestamp(&mut writer);
        writer
    }

    // Write a disconnect packet
    #[cfg(not(feature = "transport_udp"))]
    fn write_disconnect(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::Disconnect.ser(&mut writer);

        let identity_token = self.identity_token.as_ref().unwrap();
        identity_token.ser(&mut writer);

        writer
    }
}

impl HandshakeManager {
    pub fn new(
        protocol_id: ProtocolId,
        send_interval: Duration,
        ping_interval: Duration,
        handshake_pings: u8,
    ) -> Self {
        let mut handshake_timer = Timer::new(send_interval);
        handshake_timer.ring_manual();

        Self {
            protocol_id,
            handshake_timer,
            identity_token: None,
            #[cfg(feature = "transport_udp")]
            pre_connection_timestamp: stamp_time::now(),
            #[cfg(feature = "transport_udp")]
            pre_connection_digest: None,
            #[cfg(feature = "transport_udp")]
            connection_state: HandshakeState::AwaitingChallengeResponse,
            #[cfg(not(feature = "transport_udp"))]
            connection_state: HandshakeState::AwaitingIdentifyResponse,
            ping_interval,
            handshake_pings,
        }
    }

    // Step 1 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
    fn write_challenge_request(&self, identity_token: &IdentityToken) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ClientChallengeRequest(self.protocol_id).ser(&mut writer);

        self.pre_connection_timestamp.ser(&mut writer);
        identity_token.ser(&mut writer);

        writer
    }

    // Step 2 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
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

    // Step 3 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
    fn write_validate_request(&self) -> BitWriter {
        let mut writer = BitWriter::new();

        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ClientValidateRequest.ser(&mut writer);

        // write timestamp & digest into payload
        self.write_signed_timestamp(&mut writer);

        writer
    }

    // Step 4 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
    fn recv_validate_response(&mut self) {
        self.connection_state = HandshakeState::TimeSync(HandshakeTimeManager::new(
            self.ping_interval,
            self.handshake_pings,
        ));
    }

    // Step 1 of Handshake (builds without address validation)
    #[cfg(not(feature = "transport_udp"))]
    fn write_identify_request(&self, identity_token: &IdentityToken) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ClientIdentifyRequest(self.protocol_id).ser(&mut writer);

        identity_token.ser(&mut writer);

        writer
    }

    // Step 2 of Handshake (builds without address validation)
    #[cfg(not(feature = "transport_udp"))]
    fn recv_identify_response(&mut self, _reader: &mut BitReader) {
        if self.connection_state == HandshakeState::AwaitingIdentifyResponse {
            self.connection_state = HandshakeState::TimeSync(HandshakeTimeManager::new(
                self.ping_interval,
                self.handshake_pings,
            ));
        }
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
        // Ignore a connect response for a stage we are not in. The swap
        // below used to run unconditionally, clobbering an in-progress
        // identify/time-sync into Connected while reporting None.
        if !matches!(
            self.connection_state,
            HandshakeState::AwaitingConnectResponse(_)
        ) {
            return None;
        }
        let HandshakeState::AwaitingConnectResponse(time_manager) =
            std::mem::replace(&mut self.connection_state, HandshakeState::Connected)
        else {
            return None;
        };

        Some(HandshakeResult::Connected(Box::new(time_manager)))
    }

    #[cfg(feature = "transport_udp")]
    fn write_signed_timestamp(&self, writer: &mut BitWriter) {
        self.pre_connection_timestamp.ser(writer);
        let digest: &Vec<u8> = self.pre_connection_digest.as_ref().unwrap();
        digest.ser(writer);
    }
}

// Mid-handshake transition pins: timeout-driven retransmit pacing and
// wrong-stage packet handling. The retransmit tests run the timer; the
// wrong-stage tests pin that a packet for a stage we are not in is
// ignored WITHOUT moving the state machine — a stray ServerConnectResponse
// mid-handshake must not clobber the in-progress stage into Connected.
#[cfg(all(test, feature = "test_time"))]
mod tests {
    use std::time::Duration;

    use naia_shared::{
        handshake::{HandshakeHeader, RejectReason},
        BitReader, BitWriter, IdentityToken, PacketType, ProtocolId, Serde, StandardHeader,
        TestClock,
    };

    use super::{HandshakeManager, HandshakeResult, Handshaker};

    const SEND_INTERVAL_MS: u64 = 1000;

    /// Starts the thread-local TestClock at a nonzero epoch millisecond.
    /// Production clocks read epoch time, which never sits at zero; the
    /// test backend saturates `ring_manual` subtractions at zero, so a
    /// clock left at exactly 0 cannot represent "in the past". Every test
    /// below starts the clock where production lives.
    fn init_clock() {
        TestClock::init(1_000_000);
    }

    fn protocol_id() -> ProtocolId {
        ProtocolId::from_bytes([7u8; 16])
    }

    fn manager() -> HandshakeManager {
        HandshakeManager::new(
            protocol_id(),
            Duration::from_millis(SEND_INTERVAL_MS),
            Duration::from_millis(100),
            3,
        )
    }

    fn with_token(manager: &mut HandshakeManager) {
        manager.set_identity_token(IdentityToken::from_bytes(vec![9u8; 16]));
    }

    /// Serializes one inbound server packet for `recv`.
    fn inbound_bytes(packet_type: PacketType, header: Option<HandshakeHeader>) -> Vec<u8> {
        let mut writer = BitWriter::new();
        StandardHeader::new(packet_type, 0, 0, 0).ser(&mut writer);
        if let Some(header) = header {
            header.ser(&mut writer);
        }
        writer.to_packet().slice().to_vec()
    }

    fn recv_handshake(
        manager: &mut HandshakeManager,
        header: HandshakeHeader,
    ) -> Option<HandshakeResult> {
        let bytes = inbound_bytes(PacketType::Handshake, Some(header));
        let mut reader = BitReader::new(&bytes);
        manager.recv(&mut reader)
    }

    fn packet_type_of_first_send(manager: &mut HandshakeManager) -> Option<PacketType> {
        let packet = manager.send()?;
        let mut reader = BitReader::new(packet.slice());
        StandardHeader::de(&mut reader)
            .ok()
            .map(|header| header.packet_type)
    }

    #[test]
    fn retransmit_paced_by_handshake_timer() {
        init_clock();
        let mut manager = manager();
        with_token(&mut manager);

        // Constructor rings the timer manually; the backend only rings once
        // the interval has *strictly* elapsed, and frozen test time supplies
        // no epsilon the way production wall time does.
        TestClock::advance(1);
        assert_eq!(
            packet_type_of_first_send(&mut manager),
            Some(PacketType::Handshake)
        );
        // Immediately after, the interval gates us.
        assert!(manager.send().is_none());
        TestClock::advance(SEND_INTERVAL_MS - 1);
        assert!(manager.send().is_none());
        // Once the interval elapses the identify retransmits.
        TestClock::advance(2);
        assert_eq!(
            packet_type_of_first_send(&mut manager),
            Some(PacketType::Handshake)
        );
    }

    #[test]
    fn send_without_token_emits_nothing_but_keeps_stage() {
        init_clock();
        let mut manager = manager();

        TestClock::advance(SEND_INTERVAL_MS + 1);
        assert!(manager.send().is_none());
        // Setting the token later resumes the pending identify: the
        // stage was never moved by the tokenless ticks.
        with_token(&mut manager);
        TestClock::advance(SEND_INTERVAL_MS + 1);
        assert_eq!(
            packet_type_of_first_send(&mut manager),
            Some(PacketType::Handshake)
        );
    }

    #[test]
    fn mid_handshake_reject_is_terminal() {
        init_clock();
        let mut manager = manager();
        with_token(&mut manager);

        let result = recv_handshake(
            &mut manager,
            HandshakeHeader::ServerRejectResponse(RejectReason::Auth),
        );
        assert!(
            matches!(result, Some(HandshakeResult::Rejected(RejectReason::Auth))),
            "mid-handshake server reject must surface as terminal Rejected",
        );
    }

    #[test]
    fn wrong_stage_connect_response_does_not_clobber_identify() {
        init_clock();
        let mut manager = manager();
        with_token(&mut manager);

        // A stray connect response for a stage we never reached: no
        // result, and — critically — no state move.
        let result = recv_handshake(&mut manager, HandshakeHeader::ServerConnectResponse);
        assert!(result.is_none());
        TestClock::advance(SEND_INTERVAL_MS + 1);
        assert_eq!(
            packet_type_of_first_send(&mut manager),
            Some(PacketType::Handshake),
            "stray connect response must leave the identify stage intact",
        );
    }

    #[test]
    fn non_handshake_packets_ignored_mid_handshake() {
        init_clock();
        let mut manager = manager();
        with_token(&mut manager);

        for packet_type in [PacketType::Data, PacketType::Heartbeat, PacketType::Ping] {
            let bytes = inbound_bytes(packet_type, None);
            let mut reader = BitReader::new(&bytes);
            assert!(manager.recv(&mut reader).is_none());
        }
        TestClock::advance(SEND_INTERVAL_MS + 1);
        assert_eq!(
            packet_type_of_first_send(&mut manager),
            Some(PacketType::Handshake)
        );
    }

    #[test]
    fn wrong_stage_connect_response_does_not_clobber_timesync() {
        init_clock();
        let mut manager = manager();
        with_token(&mut manager);

        // Reach TimeSync with the identify response.
        let result = recv_handshake(&mut manager, HandshakeHeader::ServerIdentifyResponse);
        assert!(result.is_none());
        TestClock::advance(SEND_INTERVAL_MS + 1);
        assert_eq!(
            packet_type_of_first_send(&mut manager),
            Some(PacketType::Ping),
            "timesync stage must emit pings",
        );
        // Stray connect response mid-timesync: no result, no state move.
        let result = recv_handshake(&mut manager, HandshakeHeader::ServerConnectResponse);
        assert!(result.is_none());
        TestClock::advance(SEND_INTERVAL_MS + 1);
        assert_eq!(
            packet_type_of_first_send(&mut manager),
            Some(PacketType::Ping),
            "stray connect response must leave the timesync stage intact",
        );
    }
}
