use std::{collections::HashMap, net::SocketAddr};

use log::warn;
#[cfg(feature = "transport_udp")]
use ring::{hmac, rand};

use naia_shared::{
    handshake::{HandshakeHeader, RejectReason},
    BitReader, BitWriter, DisconnectReason, IdentityToken, OutgoingPacket, PacketType, ProtocolId,
    Serde, SerdeErr, StandardHeader,
};

#[cfg(feature = "transport_udp")]
use crate::handshake::cache_map::CacheMap;
use crate::{
    handshake::{HandshakeAction, Handshaker},
    UserKey,
};

#[cfg(feature = "transport_udp")]
type Timestamp = u64;

/// Maximum in-flight pending handshake connections held in the LRU map.
/// Sized to hold ~1 K simultaneous pre-auth connections before the LRU evicts
/// the oldest; prevents OOM from spoofed source-address floods.
#[cfg(feature = "transport_udp")]
const MAX_PENDING_CONNECTIONS: usize = 1024;

/// Number of recent handshake timestamps held in the digest replay-protection
/// LRU. 64 covers ~1 second of 60 Hz reconnects from a single client — any
/// older timestamp digests from the same client are considered expired.
#[cfg(feature = "transport_udp")]
const MAX_TIMESTAMP_DIGESTS: usize = 64;

/// The server's side of the session negotiation.
///
/// Both builds run the same negotiation: accept the client's identity token,
/// bind its source address to the already-authenticated user, serve time-sync
/// pings, then answer the connect request. Builds with source-address
/// validation (raw UDP) prepend an HMAC challenge/validate round-trip, which
/// proves the client can receive at the address it claims before the server
/// commits any per-connection state to it.
pub struct HandshakeManager {
    protocol_id: ProtocolId,
    authenticated_and_identified_users: HashMap<SocketAddr, UserKey>,
    authenticated_unidentified_users: HashMap<IdentityToken, UserKey>,
    identity_token_map: HashMap<UserKey, IdentityToken>,
    #[cfg(feature = "transport_udp")]
    been_handshaked_users: HashMap<SocketAddr, UserKey>,

    #[cfg(feature = "transport_udp")]
    connection_hash_key: hmac::Key,
    // Bounded LRU cache; caps at MAX_PENDING_CONNECTIONS to prevent OOM from
    // spoofed source-address floods before authentication completes.
    #[cfg(feature = "transport_udp")]
    address_to_timestamp_map: CacheMap<SocketAddr, Timestamp>,
    #[cfg(feature = "transport_udp")]
    timestamp_digest_map: CacheMap<Timestamp, Vec<u8>>,
}

impl Handshaker for HandshakeManager {
    fn authenticate_user(&mut self, identity_token: &IdentityToken, user_key: UserKey) {
        self.authenticated_unidentified_users
            .insert(identity_token.clone(), user_key);
        self.identity_token_map
            .insert(user_key, identity_token.clone());
    }

    fn delete_user(&mut self, user_key: UserKey, address_opt: Option<SocketAddr>) {
        if let Some(identity_token) = self.identity_token_map.remove(&user_key) {
            self.authenticated_unidentified_users
                .remove(&identity_token);
        }
        if let Some(address) = address_opt {
            self.authenticated_and_identified_users.remove(&address);
            #[cfg(feature = "transport_udp")]
            {
                self.been_handshaked_users.remove(&address);
                self.address_to_timestamp_map.remove(&address);
            }
        } else {
            // User disconnected before finalize_connection set data_addr; scan by value
            // to ensure been_handshaked_users doesn't leak on pre-finalization drops.
            #[cfg(feature = "transport_udp")]
            self.been_handshaked_users.retain(|_, v| v != user_key);
        }
    }

    fn maintain_handshake(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
        has_connection: bool,
    ) -> Result<HandshakeAction, SerdeErr> {
        let handshake_header = HandshakeHeader::de(reader)?;

        // Handshake stuff
        match handshake_header {
            #[cfg(feature = "transport_udp")]
            HandshakeHeader::ClientChallengeRequest(protocol_id) => {
                self.handle_challenge_request(address, reader, protocol_id)
            }
            #[cfg(feature = "transport_udp")]
            HandshakeHeader::ClientValidateRequest => self.handle_validate_request(address, reader),
            #[cfg(not(feature = "transport_udp"))]
            HandshakeHeader::ClientIdentifyRequest(protocol_id) => {
                Ok(self.handle_identify_request(address, reader, protocol_id, has_connection))
            }
            #[cfg(feature = "transport_udp")]
            HandshakeHeader::ClientConnectRequest => {
                self.handle_connect_request(address, has_connection)
            }
            #[cfg(not(feature = "transport_udp"))]
            HandshakeHeader::ClientConnectRequest => Ok(HandshakeAction::ForwardPacket),
            HandshakeHeader::Disconnect => {
                if self.verify_disconnect_request(address, reader) {
                    // Get the user_key for this address to disconnect
                    if let Some(user_key) = self.authenticated_and_identified_users.get(address) {
                        Ok(HandshakeAction::DisconnectUser(*user_key))
                    } else {
                        Ok(HandshakeAction::None)
                    }
                } else {
                    Ok(HandshakeAction::None)
                }
            }
            _ => {
                warn!(
                    "Server Error: Unexpected handshake header: {handshake_header:?} from {address}"
                );
                Ok(HandshakeAction::None)
            }
        }
    }

    fn reset(&mut self) {
        self.authenticated_and_identified_users.clear();
        self.authenticated_unidentified_users.clear();
        self.identity_token_map.clear();
        #[cfg(feature = "transport_udp")]
        {
            self.been_handshaked_users.clear();
            self.address_to_timestamp_map.clear();
            self.timestamp_digest_map.clear();
        }
    }

    fn write_disconnect(&self, reason: DisconnectReason, payload: Option<&[u8]>) -> OutgoingPacket {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerDisconnect(reason).ser(&mut writer);
        payload.map(<[u8]>::to_vec).ser(&mut writer);
        writer.to_packet()
    }
}

impl HandshakeManager {
    #[cfg(feature = "transport_udp")]
    /// Handle a client challenge request: reject protocol mismatches and
    /// answer identified clients with a challenge response.
    ///
    /// # Panics
    ///
    /// Panics if the identity token map lost the user's token, which
    /// cannot happen for an authenticated user.
    fn handle_challenge_request(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
        protocol_id: ProtocolId,
    ) -> Result<HandshakeAction, SerdeErr> {
        if protocol_id != self.protocol_id {
            warn!(
                "Server: Protocol Mismatch! Client: {}, Server: {}",
                protocol_id, self.protocol_id
            );
            let reject_response =
                Self::write_reject_response(RejectReason::ProtocolMismatch).to_packet();
            return Ok(HandshakeAction::SendPacket(reject_response));
        }
        if let Ok((timestamp, id_token)) = self.recv_challenge_request(reader) {
            if let Some(user_key) = self.authenticated_unidentified_users.remove(&id_token) {
                // remove identity token from map
                if self.identity_token_map.remove(&user_key).is_none() {
                    panic!("Server Error: Identity Token not found for user_key: {:?}. Shouldn't be possible.", user_key);
                }

                // User is authenticated and identified
                self.authenticated_and_identified_users
                    .insert(*address, user_key);
            } else if !self
                .authenticated_and_identified_users
                .contains_key(address)
            {
                // Unknown retry: never authenticated and never
                // finalized — stay silent as before. A retry for an
                // already-finalized address falls through and gets
                // the response re-sent below: the client retransmits
                // over an unreliable channel, so the first response
                // is routinely lost.
                return Ok(HandshakeAction::None);
            }

            let identify_response = self.write_challenge_response(&timestamp).to_packet();

            Ok(HandshakeAction::SendPacket(identify_response))
        } else {
            Ok(HandshakeAction::None)
        }
    }

    #[cfg(feature = "transport_udp")]
    /// Handle a client validate request: finish validated handshakes or
    /// answer with a validate response.
    fn handle_validate_request(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
    ) -> Result<HandshakeAction, SerdeErr> {
        if self.recv_validate_request(address, reader) {
            if self.been_handshaked_users.contains_key(address) {
                // send validate response
                let writer = self.write_validate_response();
                Ok(HandshakeAction::SendPacket(writer.to_packet()))
            } else {
                // info!("checking authenticated users for {}", address);
                if let Some(user_key) = self.authenticated_and_identified_users.get(address) {
                    let user_key = *user_key;
                    let address = *address;
                    let packet = self.user_finish_handshake(&address, &user_key);
                    Ok(HandshakeAction::SendPacket(packet))
                } else {
                    warn!("Server Error: Cannot find user by address {}", address);
                    Ok(HandshakeAction::None)
                }
            }
        } else {
            // do nothing
            Ok(HandshakeAction::None)
        }
    }

    #[cfg(not(feature = "transport_udp"))]
    /// Handle a client identify request: reject protocol mismatches and
    /// unknown identity tokens, otherwise finalize the connection.
    ///
    /// # Panics
    ///
    /// Panics if the identity token map lost the user's token, which
    /// cannot happen for an authenticated user.
    fn handle_identify_request(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
        protocol_id: ProtocolId,
        has_connection: bool,
    ) -> HandshakeAction {
        if protocol_id != self.protocol_id {
            warn!(
                "Server: Protocol Mismatch! Client: {}, Server: {}",
                protocol_id, self.protocol_id
            );
            let reject_response =
                Self::write_reject_response(RejectReason::ProtocolMismatch).to_packet();
            return HandshakeAction::SendPacket(reject_response);
        }
        if has_connection {
            let identify_response = Self::write_identity_response().to_packet();
            HandshakeAction::SendPacket(identify_response)
        } else {
            let Ok(id_token) = Self::recv_identify_request(reader) else {
                return HandshakeAction::None;
            };
            let Some(user_key) = self.authenticated_unidentified_users.remove(&id_token) else {
                // Duplicate of a consumed identify for an address
                // that already finalized: resend the response without
                // re-finalizing (finalize emits a downstream
                // connection event and must run exactly once).
                // Anything else is still an Auth reject.
                if self
                    .authenticated_and_identified_users
                    .contains_key(address)
                {
                    let identify_response = Self::write_identity_response().to_packet();
                    return HandshakeAction::SendPacket(identify_response);
                }
                let reject_response = Self::write_reject_response(RejectReason::Auth).to_packet();
                return HandshakeAction::SendPacket(reject_response);
            };
            // Verify identity token exists (but keep it for disconnect verification)
            assert!(self.identity_token_map.contains_key(&user_key), "Server Error: Identity Token not found for user_key: {user_key:?}. Shouldn't be possible.");

            // User is authenticated
            self.authenticated_and_identified_users
                .insert(*address, user_key);

            // send identify response
            let identify_response = Self::write_identity_response().to_packet();
            HandshakeAction::FinalizeConnection(user_key, identify_response)
        }
    }

    #[cfg(feature = "transport_udp")]
    /// Handle a client connect request: finalize a validated connection.
    ///
    /// # Panics
    ///
    /// Panics if the address was never validated, which the validation
    /// step guarantees.
    fn handle_connect_request(
        &mut self,
        address: &SocketAddr,
        has_connection: bool,
    ) -> Result<HandshakeAction, SerdeErr> {
        // send connect response
        let writer = Self::write_connect_response();
        let packet = writer.to_packet();

        if has_connection {
            Ok(HandshakeAction::SendPacket(packet))
        } else {
            let user_key = *self
                .been_handshaked_users
                .get(address)
                .expect("should be a user by now, from validation step");

            Ok(HandshakeAction::FinalizeConnection(user_key, packet))
        }
    }

    pub fn new(protocol_id: ProtocolId) -> Self {
        #[cfg(feature = "transport_udp")]
        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        Self {
            protocol_id,
            authenticated_and_identified_users: HashMap::new(),
            authenticated_unidentified_users: HashMap::new(),
            identity_token_map: HashMap::new(),
            #[cfg(feature = "transport_udp")]
            been_handshaked_users: HashMap::new(),

            #[cfg(feature = "transport_udp")]
            connection_hash_key,
            #[cfg(feature = "transport_udp")]
            address_to_timestamp_map: CacheMap::with_capacity(MAX_PENDING_CONNECTIONS),
            #[cfg(feature = "transport_udp")]
            timestamp_digest_map: CacheMap::with_capacity(MAX_TIMESTAMP_DIGESTS),
        }
    }

    // Step 1 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
    fn recv_challenge_request(
        &mut self,
        reader: &mut BitReader,
    ) -> Result<(Timestamp, IdentityToken), SerdeErr> {
        let timestamp = Timestamp::de(reader)?;
        let identity_token = IdentityToken::de(reader)?;

        Ok((timestamp, identity_token))
    }

    // Step 2 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
    fn write_challenge_response(&mut self, timestamp: &Timestamp) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerChallengeResponse.ser(&mut writer);
        timestamp.ser(&mut writer);

        if !self.timestamp_digest_map.contains_key(timestamp) {
            let tag = hmac::sign(&self.connection_hash_key, &timestamp.to_le_bytes());
            let tag_vec: Vec<u8> = Vec::from(tag.as_ref());
            self.timestamp_digest_map.insert(*timestamp, tag_vec);
        }

        //write timestamp digest
        self.timestamp_digest_map
            .get_unchecked(timestamp)
            .ser(&mut writer);

        writer
    }

    // Step 3 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
    fn recv_validate_request(&mut self, address: &SocketAddr, reader: &mut BitReader) -> bool {
        // Verify that timestamp hash has been written by this
        // server instance
        let Some(timestamp) = self.timestamp_validate(reader) else {
            warn!("Handshake Error from {}: Invalid timestamp hash", address);
            return false;
        };
        // Timestamp hash is valid

        self.address_to_timestamp_map.insert(*address, timestamp);

        true
    }

    // Step 4 of Handshake (address-validating builds)
    #[cfg(feature = "transport_udp")]
    fn write_validate_response(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerValidateResponse.ser(&mut writer);
        writer
    }

    // Step 1 of Handshake (builds without address validation)
    #[cfg(not(feature = "transport_udp"))]
    fn recv_identify_request(reader: &mut BitReader) -> Result<IdentityToken, SerdeErr> {
        IdentityToken::de(reader)
    }

    // Step 2 of Handshake (builds without address validation)
    #[cfg(not(feature = "transport_udp"))]
    fn write_identity_response() -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerIdentifyResponse.ser(&mut writer);

        writer
    }

    // Step 5 of Handshake
    pub(crate) fn write_connect_response() -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerConnectResponse.ser(&mut writer);
        writer
    }

    /// Address-validating builds verify the disconnect's signed timestamp
    /// against the one recorded for this address at validation time.
    #[cfg(feature = "transport_udp")]
    fn verify_disconnect_request(&mut self, address: &SocketAddr, reader: &mut BitReader) -> bool {
        if let Some(new_timestamp) = self.timestamp_validate(reader) {
            if let Some(old_timestamp) = self.address_to_timestamp_map.get(address) {
                if *old_timestamp == new_timestamp {
                    return true;
                }
            }
        }

        false
    }

    /// Builds without address validation verify the disconnect by comparing
    /// the identity token it carries against the one minted for this user.
    #[cfg(not(feature = "transport_udp"))]
    fn verify_disconnect_request(&mut self, address: &SocketAddr, reader: &mut BitReader) -> bool {
        // Read the identity token from the disconnect packet
        let Ok(disconnect_token) = IdentityToken::de(reader) else {
            return false;
        };

        // Verify the address is authenticated
        let Some(user_key) = self.authenticated_and_identified_users.get(address) else {
            return false;
        };

        // Verify the identity token matches what we expect for this user
        let Some(expected_token) = self.identity_token_map.get(user_key) else {
            return false;
        };

        // Token must match
        *expected_token == disconnect_token
    }

    fn write_reject_response(reason: RejectReason) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerRejectResponse(reason).ser(&mut writer);
        writer
    }

    #[cfg(feature = "transport_udp")]
    fn timestamp_validate(&self, reader: &mut BitReader) -> Option<Timestamp> {
        // Read timestamp
        let timestamp_result = Timestamp::de(reader);
        if timestamp_result.is_err() {
            return None;
        }
        let timestamp = timestamp_result.unwrap();

        // Read digest
        let digest_bytes_result = Vec::<u8>::de(reader);
        if digest_bytes_result.is_err() {
            return None;
        }
        let digest_bytes = digest_bytes_result.unwrap();

        // Verify that timestamp hash has been written by this server instance
        let validation_result = hmac::verify(
            &self.connection_hash_key,
            &timestamp.to_le_bytes(),
            &digest_bytes,
        );
        if validation_result.is_err() {
            None
        } else {
            Some(timestamp)
        }
    }

    #[cfg(feature = "transport_udp")]
    fn user_finish_handshake(&mut self, addr: &SocketAddr, user_key: &UserKey) -> OutgoingPacket {
        // send validate response
        let writer = self.write_validate_response();
        let packet = writer.to_packet();

        self.been_handshaked_users.insert(*addr, *user_key);

        packet
    }
}

// Server reject-production pins: the reject packets whose wire encoding
// (shared) and terminal handling (client) are already pinned must
// actually be produced here with the right reason. This module is the
// server handshaker's first unit coverage — no sockets involved, just the
// state machine over crafted inbound bytes.
#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use naia_shared::BigMapKey;

    use super::*;

    fn addr() -> SocketAddr {
        "127.0.0.1:9999".parse().unwrap()
    }

    fn server_pid() -> ProtocolId {
        ProtocolId::from_bytes([7u8; 16])
    }

    fn wrong_pid() -> ProtocolId {
        ProtocolId::from_bytes([8u8; 16])
    }

    /// Inbound handshake bytes: header plus an optional trailing identity
    /// token, the way the transport delivers them after the standard
    /// header (which `maintain_handshake` never sees).
    fn inbound(header: &HandshakeHeader, token: Option<&IdentityToken>) -> Vec<u8> {
        let mut writer = BitWriter::new();
        header.ser(&mut writer);
        if let Some(token) = token {
            token.ser(&mut writer);
        }
        writer.to_packet().slice().to_vec()
    }

    fn maintain(manager: &mut HandshakeManager, bytes: &[u8]) -> Result<HandshakeAction, SerdeErr> {
        let mut reader = BitReader::new(bytes);
        manager.maintain_handshake(&addr(), &mut reader, false)
    }

    /// A produced reject packet must parse as the named reason — this is
    /// the producer half of the wire+consumer pins.
    fn reject_reason_of(packet: &OutgoingPacket) -> RejectReason {
        let mut reader = BitReader::new(packet.slice());
        StandardHeader::de(&mut reader).expect("reject packet must parse");
        match HandshakeHeader::de(&mut reader).expect("reject header must parse") {
            HandshakeHeader::ServerRejectResponse(reason) => reason,
            other => panic!("expected a reject packet, got {other:?}"),
        }
    }

    #[cfg(feature = "transport_udp")]
    #[test]
    fn challenge_wrong_protocol_emits_mismatch_reject() {
        let mut manager = HandshakeManager::new(server_pid());
        let bytes = inbound(&HandshakeHeader::ClientChallengeRequest(wrong_pid()), None);
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::SendPacket(packet)) => {
                assert_eq!(reject_reason_of(&packet), RejectReason::ProtocolMismatch,)
            }
            _ => panic!("wrong protocol must be rejected with a packet"),
        }
    }

    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn identify_wrong_protocol_emits_mismatch_reject() {
        let mut manager = HandshakeManager::new(server_pid());
        let bytes = inbound(&HandshakeHeader::ClientIdentifyRequest(wrong_pid()), None);
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::SendPacket(packet)) => {
                assert_eq!(reject_reason_of(&packet), RejectReason::ProtocolMismatch,)
            }
            _ => panic!("wrong protocol must be rejected with a packet"),
        }
    }

    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn identify_unknown_token_emits_auth_reject() {
        // A well-formed identify for a token the server never authenticated
        // is a credential failure, not a protocol failure.
        let mut manager = HandshakeManager::new(server_pid());
        let token = IdentityToken::generate();
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&token),
        );
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::SendPacket(packet)) => {
                assert_eq!(reject_reason_of(&packet), RejectReason::Auth)
            }
            _ => panic!("unknown token must be auth-rejected with a packet"),
        }
    }

    /// A client retransmit of identify after a dropped server response must
    /// be answered again, never rejected. The client resends every
    /// send_handshake_interval over an unreliable channel, so the first
    /// response is routinely lost; the retry finds the connection finalized
    /// and must observe it as acknowledged.
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn duplicate_identify_after_finalize_resends_response() {
        let mut manager = HandshakeManager::new(server_pid());
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, &user_key);
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&token),
        );
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::FinalizeConnection(finalized_key, _)) => {
                assert_eq!(finalized_key, user_key)
            }
            _ => panic!("first identify must finalize the connection"),
        }
        // The response was dropped on the wire; the client's retransmit
        // arrives with the connection already finalized.
        let mut reader = BitReader::new(&bytes);
        match manager.maintain_handshake(&addr(), &mut reader, true) {
            Ok(HandshakeAction::SendPacket(packet)) => {
                let mut reader = BitReader::new(packet.slice());
                StandardHeader::de(&mut reader).expect("response must parse");
                assert!(
                    matches!(
                        HandshakeHeader::de(&mut reader),
                        Ok(HandshakeHeader::ServerIdentifyResponse)
                    ),
                    "duplicate identify must resend the identify response"
                );
            }
            _ => panic!("duplicate identify must resend the response, never reject"),
        }
    }

    /// A duplicate identify that arrives while the transport holds no
    /// connection record must still be answered, never Auth-rejected. The
    /// token was consumed by the first pass, but the user is finalized; a
    /// reject here kills a live handshake the client is still retrying.
    /// Re-sends the response without re-finalizing (finalize emits a
    /// connection event downstream and must run exactly once).
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn duplicate_identify_without_connection_resends_response() {
        let mut manager = HandshakeManager::new(server_pid());
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, &user_key);
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&token),
        );
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::FinalizeConnection(finalized_key, _)) => {
                assert_eq!(finalized_key, user_key)
            }
            _ => panic!("first identify must finalize the connection"),
        }
        // Same bytes again, transport still holding no connection record.
        let mut reader = BitReader::new(&bytes);
        match manager.maintain_handshake(&addr(), &mut reader, false) {
            Ok(HandshakeAction::SendPacket(packet)) => {
                let mut reader = BitReader::new(packet.slice());
                StandardHeader::de(&mut reader).expect("response must parse");
                assert!(
                    matches!(
                        HandshakeHeader::de(&mut reader),
                        Ok(HandshakeHeader::ServerIdentifyResponse)
                    ),
                    "duplicate identify must resend the identify response, never reject"
                );
            }
            _ => panic!("duplicate identify must resend the response, never reject"),
        }
    }

    /// Challenge request bytes: header, timestamp, then token — the exact
    /// layout `recv_challenge_request` reads.
    #[cfg(feature = "transport_udp")]
    fn challenge_inbound(timestamp: u64, token: &IdentityToken) -> Vec<u8> {
        let mut writer = BitWriter::new();
        HandshakeHeader::ClientChallengeRequest(server_pid()).ser(&mut writer);
        timestamp.ser(&mut writer);
        token.ser(&mut writer);
        writer.to_packet().slice().to_vec()
    }

    /// A client retransmit of the challenge request after a dropped server
    /// response must be answered with the identical response bytes, never
    /// silence. The first pass consumes the token; the retry must observe
    /// the finalized user and re-derive the (timestamp-deterministic)
    /// response instead of returning None.
    #[cfg(feature = "transport_udp")]
    #[test]
    fn duplicate_challenge_resends_response() {
        let mut manager = HandshakeManager::new(server_pid());
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, &user_key);
        let bytes = challenge_inbound(12345, &token);
        let first = match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::SendPacket(packet)) => packet,
            _ => panic!("first challenge must send the challenge response"),
        };
        // The response was dropped on the wire; the client retransmits the
        // identical request bytes.
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::SendPacket(packet)) => {
                assert_eq!(
                    packet.slice(),
                    first.slice(),
                    "duplicate challenge must resend the identical response bytes"
                );
            }
            _ => panic!("duplicate challenge must resend the response, never go silent"),
        }
    }
}
