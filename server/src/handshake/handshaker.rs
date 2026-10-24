use std::{collections::HashMap, net::SocketAddr, time::Duration};

use log::warn;
#[cfg(feature = "transport_udp")]
use ring::{hmac, rand};

use naia_shared::{
    handshake::{HandshakeHeader, RejectReason},
    BitReader, BitWriter, DisconnectReason, IdentityToken, Instant, OutgoingPacket, PacketType,
    ProtocolId, Serde, SerdeErr, StandardHeader,
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
    /// Grants the server has issued but no client has presented yet, with
    /// the instant each grant was issued so the TTL can expire them
    /// ([connection-10]).
    authenticated_unidentified_users: HashMap<IdentityToken, (UserKey, Instant)>,
    identity_token_map: HashMap<UserKey, IdentityToken>,
    /// How long an issued token stays valid for the identify step. From
    /// `ServerConfig::identity_token_ttl`.
    identity_token_ttl: Duration,
    /// Source address of each accepted-but-unidentified handshake, so a
    /// failed identify can resolve back to the user record it must delete
    /// ([connection-24]). Removed at finalize, disconnect, or reset.
    unidentified_addresses: HashMap<SocketAddr, UserKey>,
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
    /// Client handshake pings received per source address, both builds:
    /// challenge receipts on address-validating builds, identify receipts
    /// otherwise. Taken (removed) when the handshake finalizes; backs the
    /// `handshake_attempts` observer that tells first-send from resend.
    ping_counts: HashMap<SocketAddr, u32>,
}

impl Handshaker for HandshakeManager {
    fn authenticate_user(
        &mut self,
        identity_token: &IdentityToken,
        user_key: UserKey,
        address: &SocketAddr,
    ) {
        self.authenticated_unidentified_users
            .insert(identity_token.clone(), (user_key, Instant::now()));
        self.identity_token_map
            .insert(user_key, identity_token.clone());
        self.unidentified_addresses.insert(*address, user_key);
    }

    fn delete_user(&mut self, user_key: UserKey, address_opt: Option<SocketAddr>) {
        if let Some(identity_token) = self.identity_token_map.remove(&user_key) {
            self.authenticated_unidentified_users
                .remove(&identity_token);
        }
        self.unidentified_addresses
            .retain(|_, pending_key| *pending_key != user_key);
        if let Some(address) = address_opt {
            self.authenticated_and_identified_users.remove(&address);
            self.ping_counts.remove(&address);
            #[cfg(feature = "transport_udp")]
            {
                self.been_handshaked_users.remove(&address);
                self.address_to_timestamp_map.remove(&address);
            }
        } else {
            // User disconnected before finalize_connection set data_addr; scan by value
            // to ensure been_handshaked_users doesn't leak on pre-finalization drops.
            #[cfg(feature = "transport_udp")]
            self.been_handshaked_users.retain(|_, v| *v != user_key);
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

    fn take_handshake_attempts(&mut self, address: &SocketAddr) -> Option<u32> {
        self.take_handshake_attempts(address)
    }

    fn reset(&mut self) {
        self.authenticated_and_identified_users.clear();
        self.authenticated_unidentified_users.clear();
        self.identity_token_map.clear();
        self.unidentified_addresses.clear();
        self.ping_counts.clear();
        #[cfg(feature = "transport_udp")]
        {
            self.been_handshaked_users.clear();
            self.address_to_timestamp_map.clear();
            self.timestamp_digest_map.clear();
        }
    }

    fn sweep_expired_identity_tokens(&mut self) {
        let now = Instant::now();
        let ttl = self.identity_token_ttl;
        let expired: Vec<IdentityToken> = self
            .authenticated_unidentified_users
            .iter()
            .filter(|(_, (_, issued_at))| issued_at.elapsed(&now) > ttl)
            .map(|(token, _)| token.clone())
            .collect();
        for token in expired {
            if let Some((user_key, _)) = self.authenticated_unidentified_users.remove(&token) {
                // The identity map still points at the revoked grant; drop
                // it only if it names this token, so a newer grant for the
                // same user (re-auth) is never disturbed.
                if self.identity_token_map.get(&user_key) == Some(&token) {
                    self.identity_token_map.remove(&user_key);
                }
            }
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
        *self.ping_counts.entry(*address).or_default() += 1;
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
            // A token older than its TTL is dropped exactly like a token
            // the server never issued ([connection-10]); the stale entry
            // was already removed by the lookup.
            match self.authenticated_unidentified_users.remove(&id_token) {
                Some((user_key, issued_at))
                    if issued_at.elapsed(&Instant::now()) <= self.identity_token_ttl =>
                {
                    // remove identity token from map
                    if self.identity_token_map.remove(&user_key).is_none() {
                        panic!("Server Error: Identity Token not found for user_key: {:?}. Shouldn't be possible.", user_key);
                    }

                    // User is authenticated and identified
                    self.authenticated_and_identified_users
                        .insert(*address, user_key);
                }
                _ => {
                    if !self
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
                }
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
        *self.ping_counts.entry(*address).or_default() += 1;
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
            // A token older than its TTL is refused exactly like a token
            // the server never issued ([connection-10]); the stale entry
            // was already removed by the lookup.
            let user_key = match self.authenticated_unidentified_users.remove(&id_token) {
                Some((user_key, issued_at))
                    if issued_at.elapsed(&Instant::now()) <= self.identity_token_ttl =>
                {
                    Some(user_key)
                }
                _ => None,
            };
            let Some(user_key) = user_key else {
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
                // Unknown-token identify ([connection-24]): the accept created
                // an authentication record for this address, but the client
                // cannot prove ownership of any token. Delete the never-
                // connected user so no half-connected state persists.
                let reject_response = Self::write_reject_response(RejectReason::Auth).to_packet();
                if let Some(pending_key) = self.unidentified_addresses.remove(address) {
                    return HandshakeAction::RejectUser(pending_key, reject_response);
                }
                return HandshakeAction::SendPacket(reject_response);
            };
            // Verify identity token exists (but keep it for disconnect verification)
            assert!(self.identity_token_map.contains_key(&user_key), "Server Error: Identity Token not found for user_key: {user_key:?}. Shouldn't be possible.");

            // User is authenticated
            self.authenticated_and_identified_users
                .insert(*address, user_key);
            self.unidentified_addresses.remove(address);

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

    pub fn new(protocol_id: ProtocolId, identity_token_ttl: Duration) -> Self {
        #[cfg(feature = "transport_udp")]
        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        Self {
            protocol_id,
            authenticated_and_identified_users: HashMap::new(),
            authenticated_unidentified_users: HashMap::new(),
            identity_token_map: HashMap::new(),
            identity_token_ttl,
            unidentified_addresses: HashMap::new(),
            #[cfg(feature = "transport_udp")]
            been_handshaked_users: HashMap::new(),

            #[cfg(feature = "transport_udp")]
            connection_hash_key,
            #[cfg(feature = "transport_udp")]
            address_to_timestamp_map: CacheMap::with_capacity(MAX_PENDING_CONNECTIONS),
            #[cfg(feature = "transport_udp")]
            timestamp_digest_map: CacheMap::with_capacity(MAX_TIMESTAMP_DIGESTS),
            ping_counts: HashMap::new(),
        }
    }

    /// Take the recorded client-ping count for `address`, removing it.
    /// The main server calls this when the handshake finalizes so the
    /// per-key `handshake_attempts` observer can tell the client's first
    /// send (1) from a 250 ms resend (>1). Returns `None` when no ping
    /// was ever recorded — finalize always follows a counted receipt, so
    /// `None` here means the entry was already taken or evicted.
    pub fn take_handshake_attempts(&mut self, address: &SocketAddr) -> Option<u32> {
        self.ping_counts.remove(address)
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
    use std::{net::SocketAddr, time::Duration};

    use naia_shared::BigMapKey;
    #[cfg(feature = "test_time")]
    use naia_shared::TestClock;

    use super::*;

    /// TTL all test managers mint with: large enough that wall-clock test
    /// runs never trip it, small enough that virtual-clock tests can.
    const TEST_TOKEN_TTL: Duration = Duration::from_secs(60);

    fn manager_with_ttl() -> HandshakeManager {
        HandshakeManager::new(server_pid(), TEST_TOKEN_TTL)
    }

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
        let mut manager = manager_with_ttl();
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
        let mut manager = manager_with_ttl();
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
        let mut manager = manager_with_ttl();
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

    /// An unknown-token identify for an address the server accepted both
    /// rejects on the wire and marks the never-connected user for
    /// deletion, so a failed handshake leaves no half-connected
    /// authentication state ([connection-24]).
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn identify_unknown_token_rejects_pending_user_for_deletion() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
        let tampered = IdentityToken::generate();
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&tampered),
        );
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::RejectUser(rejected_key, packet)) => {
                assert_eq!(rejected_key, user_key);
                assert_eq!(reject_reason_of(&packet), RejectReason::Auth);
            }
            _ => panic!("unknown token must reject the pending user with a packet"),
        }
    }

    /// An identity token presented after its TTL must be refused exactly
    /// like a token the server never issued: Auth reject on the wire and
    /// the pending user marked for deletion, with the stale entry dropped
    /// ([connection-10], [connection-11] expired).
    #[cfg(feature = "test_time")]
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn expired_token_at_identify_is_rejected_like_unknown() {
        TestClock::init(0);
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
        TestClock::advance(61_000);
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&token),
        );
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::RejectUser(rejected_key, packet)) => {
                assert_eq!(rejected_key, user_key);
                assert_eq!(reject_reason_of(&packet), RejectReason::Auth);
            }
            _ => panic!("expired token must be auth-rejected like an unknown token"),
        }
        assert!(
            !manager
                .authenticated_unidentified_users
                .contains_key(&token),
            "expired token entry must be dropped on refusal"
        );
        TestClock::reset();
    }

    /// A token presented inside its TTL still finalizes exactly once: the
    /// TTL machinery must not break the normal one-time-use path.
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn fresh_token_at_identify_finalizes_once() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&token),
        );
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::FinalizeConnection(finalized_key, _)) => {
                assert_eq!(finalized_key, user_key)
            }
            _ => panic!("fresh token must finalize the connection"),
        }
        assert!(
            !manager
                .authenticated_unidentified_users
                .contains_key(&token),
            "consumed token must leave no unidentified entry"
        );
    }

    /// A successful identify clears the pending-address record, so a
    /// later retry for the same finalized address resends the response
    /// instead of resolving to a stale pending user.
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn successful_identify_clears_pending_address() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&token),
        );
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::FinalizeConnection(finalized_key, _)) => {
                assert_eq!(finalized_key, user_key)
            }
            _ => panic!("known token must finalize the connection"),
        }
        assert!(
            !manager.unidentified_addresses.contains_key(&addr()),
            "finalized address must leave no pending-address record"
        );
    }

    /// Deleting a never-finalized user also clears its pending-address
    /// record, so a pre-finalization drop leaks no handshake state.
    #[test]
    fn delete_user_clears_pending_address() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
        manager.delete_user(user_key, None);
        assert!(
            !manager.unidentified_addresses.contains_key(&addr()),
            "deleted user must leave no pending-address record"
        );
    }

    /// A client retransmit of identify after a dropped server response must
    /// be answered again, never rejected. The client resends every
    /// send_handshake_interval over an unreliable channel, so the first
    /// response is routinely lost; the retry finds the connection finalized
    /// and must observe it as acknowledged.
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn duplicate_identify_after_finalize_resends_response() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
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
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
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

    /// An identity token presented at the challenge step after its TTL
    /// must be dropped exactly like a token the server never issued:
    /// silence on the wire, with the stale entry dropped
    /// ([connection-10], [connection-11] expired).
    #[cfg(feature = "test_time")]
    #[cfg(feature = "transport_udp")]
    #[test]
    fn expired_token_at_challenge_is_dropped_like_unknown() {
        TestClock::init(0);
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        manager.authenticate_user(&token, UserKey::from_u64(7), &addr());
        TestClock::advance(61_000);
        let bytes = challenge_inbound(12345, &token);
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::None) => {}
            _ => panic!("expired token challenge must be dropped like an unknown token"),
        }
        assert!(
            !manager
                .authenticated_unidentified_users
                .contains_key(&token),
            "expired token entry must be dropped on refusal"
        );
        TestClock::reset();
    }

    /// A token presented at the challenge step inside its TTL still
    /// answers with the challenge response: the TTL machinery must not
    /// break the normal address-validation path.
    #[cfg(feature = "transport_udp")]
    #[test]
    fn fresh_token_at_challenge_answers() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        manager.authenticate_user(&token, UserKey::from_u64(7), &addr());
        let bytes = challenge_inbound(12345, &token);
        match maintain(&mut manager, &bytes) {
            Ok(HandshakeAction::SendPacket(_)) => {}
            _ => panic!("fresh token challenge must send the challenge response"),
        }
    }

    /// A client retransmit of the challenge request after a dropped server
    /// response must be answered with the identical response bytes, never
    /// silence. The first pass consumes the token; the retry must observe
    /// the finalized user and re-derive the (timestamp-deterministic)
    /// response instead of returning None.
    #[cfg(feature = "transport_udp")]
    #[test]
    fn duplicate_challenge_resends_response() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
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

    /// Client pings are counted per address: a challenge plus its
    /// retransmit take as 2, and the take removes the entry. First-send
    /// vs resend reads off this count at finalize.
    #[cfg(feature = "transport_udp")]
    #[test]
    fn challenge_pings_are_counted_and_taken_once() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
        let bytes = challenge_inbound(12345, &token);
        assert!(maintain(&mut manager, &bytes).is_ok());
        assert!(maintain(&mut manager, &bytes).is_ok());
        assert_eq!(manager.take_handshake_attempts(&addr()), Some(2));
        assert_eq!(manager.take_handshake_attempts(&addr()), None);
    }

    /// Identify receipts are counted per address on builds without
    /// address validation: a single identify takes as 1.
    #[cfg(not(feature = "transport_udp"))]
    #[test]
    fn identify_pings_are_counted_and_taken_once() {
        let mut manager = manager_with_ttl();
        let token = IdentityToken::generate();
        let user_key = UserKey::from_u64(7);
        manager.authenticate_user(&token, user_key, &addr());
        let bytes = inbound(
            &HandshakeHeader::ClientIdentifyRequest(server_pid()),
            Some(&token),
        );
        assert!(maintain(&mut manager, &bytes).is_ok());
        assert_eq!(manager.take_handshake_attempts(&addr()), Some(1));
        assert_eq!(manager.take_handshake_attempts(&addr()), None);
    }

    /// The sweep drops token grants older than the TTL from both token
    /// maps, so issued-but-never-identified tokens cannot grow server
    /// memory without bound — while a token issued after the clock moved
    /// on survives the same sweep ([connection-10]).
    #[cfg(feature = "test_time")]
    #[test]
    fn sweep_removes_only_expired_token_entries() {
        TestClock::init(0);
        let mut manager = manager_with_ttl();
        let old_token = IdentityToken::generate();
        let old_key = UserKey::from_u64(7);
        manager.authenticate_user(&old_token, old_key, &addr());
        TestClock::advance(61_000);
        let fresh_token = IdentityToken::generate();
        let fresh_key = UserKey::from_u64(8);
        manager.authenticate_user(&fresh_token, fresh_key, &addr());
        manager.sweep_expired_identity_tokens();
        assert!(
            !manager
                .authenticated_unidentified_users
                .contains_key(&old_token),
            "swept token must be absent from the unidentified map"
        );
        assert!(
            !manager.identity_token_map.contains_key(&old_key),
            "swept token must be absent from the identity map"
        );
        assert!(
            manager
                .authenticated_unidentified_users
                .contains_key(&fresh_token),
            "fresh token must survive the sweep"
        );
        assert_eq!(
            manager.identity_token_map.get(&fresh_key),
            Some(&fresh_token),
            "fresh token identity record must survive the sweep"
        );
        TestClock::reset();
    }

    /// Taking attempts for an address that never pinged returns None.
    #[test]
    fn taking_attempts_for_unknown_address_returns_none() {
        let mut manager = manager_with_ttl();
        assert_eq!(manager.take_handshake_attempts(&addr()), None);
    }
}
