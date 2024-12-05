use std::{collections::HashMap, net::SocketAddr};

use log::warn;
use ring::{hmac, rand};

use naia_shared::{
    handshake::HandshakeHeader, BitReader, BitWriter, OutgoingPacket, PacketType, Serde, SerdeErr,
    StandardHeader,
};

use crate::{
    handshake::{cache_map::CacheMap, HandshakeAction, Handshaker},
    UserKey,
};

type Timestamp = u64;
type IdentityToken = String;

pub struct HandshakeManager {
    authenticated_and_identified_users: HashMap<SocketAddr, UserKey>,
    authenticated_unidentified_users: HashMap<IdentityToken, UserKey>,
    identity_token_map: HashMap<UserKey, IdentityToken>,
    been_handshaked_users: HashMap<SocketAddr, UserKey>,

    connection_hash_key: hmac::Key,
    address_to_timestamp_map: HashMap<SocketAddr, Timestamp>,
    timestamp_digest_map: CacheMap<Timestamp, Vec<u8>>,
}

impl Handshaker for HandshakeManager {
    fn authenticate_user(&mut self, identity_token: &IdentityToken, user_key: &UserKey) {
        self.authenticated_unidentified_users
            .insert(identity_token.clone(), *user_key);
        self.identity_token_map
            .insert(*user_key, identity_token.clone());
    }

    // address is optional because user may not have been identified yet
    fn delete_user(&mut self, user_key: &UserKey, address_opt: Option<SocketAddr>) {
        if let Some(identity_token) = self.identity_token_map.remove(user_key) {
            self.authenticated_unidentified_users
                .remove(&identity_token);
        }
        if let Some(address) = address_opt {
            self.authenticated_and_identified_users.remove(&address);
            self.been_handshaked_users.remove(&address);
            self.address_to_timestamp_map.remove(&address);
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
            HandshakeHeader::ClientChallengeRequest => {
                if let Ok((timestamp, id_token)) = self.recv_challenge_request(reader) {
                    if let Some(user_key) = self.authenticated_unidentified_users.remove(&id_token)
                    {
                        // remove identity token from map
                        if self.identity_token_map.remove(&user_key).is_none() {
                            panic!("Server Error: Identity Token not found for user_key: {:?}. Shouldn't be possible.", user_key);
                        }

                        // User is authenticated and identified
                        self.authenticated_and_identified_users
                            .insert(*address, user_key);
                    } else {
                        // commented out because it's pretty common to get multiple ClientChallengeRequest which would trigger this
                        //warn!("Server Error: User not authenticated for: {:?}, with token: {}", address, identity_token);

                        return Ok(HandshakeAction::None);
                    }

                    let identify_response = self.write_challenge_response(&timestamp).to_packet();

                    return Ok(HandshakeAction::SendPacket(identify_response));
                } else {
                    return Ok(HandshakeAction::None);
                }
            }
            HandshakeHeader::ClientValidateRequest => {
                if self.recv_validate_request(address, reader) {
                    if self.been_handshaked_users.contains_key(address) {
                        // send validate response
                        let writer = self.write_validate_response();
                        return Ok(HandshakeAction::SendPacket(writer.to_packet()));
                    } else {
                        // info!("checking authenticated users for {}", address);
                        if let Some(user_key) = self.authenticated_and_identified_users.get(address)
                        {
                            let user_key = *user_key;
                            let address = *address;
                            let packet = self.user_finish_handshake(&address, &user_key);
                            return Ok(HandshakeAction::SendPacket(packet));
                        } else {
                            warn!("Server Error: Cannot find user by address {}", address);
                            return Ok(HandshakeAction::None);
                        }
                    }
                } else {
                    // do nothing
                    return Ok(HandshakeAction::None);
                }
            }
            HandshakeHeader::ClientConnectRequest => {
                // send connect response
                let writer = self.write_connect_response();
                let packet = writer.to_packet();

                if has_connection {
                    return Ok(HandshakeAction::SendPacket(packet));
                } else {
                    let user_key = *self
                        .been_handshaked_users
                        .get(address)
                        .expect("should be a user by now, from validation step");

                    return Ok(HandshakeAction::FinalizeConnection(user_key, packet));
                }
            }
            HandshakeHeader::Disconnect => {
                if self.verify_disconnect_request(address, reader) {
                    let user_key = *self
                        .been_handshaked_users
                        .get(address)
                        .expect("should be a user by now, from validation step");
                    return Ok(HandshakeAction::DisconnectUser(user_key));
                } else {
                    return Ok(HandshakeAction::None);
                }
            }
            _ => {
                warn!(
                    "Server Error: Unexpected handshake header: {:?} from {}",
                    handshake_header, address
                );
                return Ok(HandshakeAction::None);
            }
        }
    }
}

impl HandshakeManager {
    pub fn new() -> Self {
        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        Self {
            authenticated_and_identified_users: HashMap::new(),
            authenticated_unidentified_users: HashMap::new(),
            identity_token_map: HashMap::new(),
            been_handshaked_users: HashMap::new(),

            connection_hash_key,
            address_to_timestamp_map: HashMap::new(),
            timestamp_digest_map: CacheMap::with_capacity(64),
        }
    }

    // Step 1 of Handshake
    fn recv_challenge_request(
        &mut self,
        reader: &mut BitReader,
    ) -> Result<(Timestamp, IdentityToken), SerdeErr> {
        let timestamp = Timestamp::de(reader)?;
        let identity_token = IdentityToken::de(reader)?;

        Ok((timestamp, identity_token))
    }

    // Step 2 of Handshake
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

    // Step 3 of Handshake
    fn recv_validate_request(&mut self, address: &SocketAddr, reader: &mut BitReader) -> bool {
        // Verify that timestamp hash has been written by this
        // server instance
        let Some(timestamp) = self.timestamp_validate(reader) else {
            warn!("Handshake Error from {}: Invalid timestamp hash", address);
            return false;
        };
        // Timestamp hash is valid

        self.address_to_timestamp_map.insert(*address, timestamp);

        return true;
    }

    // Step 4 of Handshake
    fn write_validate_response(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerValidateResponse.ser(&mut writer);
        writer
    }

    // Step 5 of Handshake
    fn write_connect_response(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerConnectResponse.ser(&mut writer);
        writer
    }

    fn verify_disconnect_request(&mut self, address: &SocketAddr, reader: &mut BitReader) -> bool {
        // Verify that timestamp hash has been written by this
        // server instance
        if let Some(new_timestamp) = self.timestamp_validate(reader) {
            if let Some(old_timestamp) = self.address_to_timestamp_map.get(address) {
                if *old_timestamp == new_timestamp {
                    return true;
                }
            }
        }

        false
    }

    // fn write_reject_response(&self) -> BitWriter {
    //     let mut writer = BitWriter::new();
    //     StandardHeader::new(PacketType::ServerRejectResponse, 0, 0, 0).ser(&mut writer);
    //     writer
    // }

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

    fn user_finish_handshake(&mut self, addr: &SocketAddr, user_key: &UserKey) -> OutgoingPacket {
        // send validate response
        let writer = self.write_validate_response();
        let packet = writer.to_packet();

        self.been_handshaked_users.insert(*addr, *user_key);

        packet
    }
}
