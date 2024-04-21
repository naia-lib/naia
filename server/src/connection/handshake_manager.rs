use std::{collections::HashMap, hash::Hash, net::SocketAddr};

use log::{info, warn};

use ring::{hmac, rand};

pub use naia_shared::{
    BitReader, BitWriter, PacketType, Serde,
    SerdeErr, StandardHeader,
};

use crate::{time_manager::TimeManager, handshake::HandshakeAction, cache_map::CacheMap, connection::{connection::Connection, io::Io}, UserKey};

pub type Timestamp = u64;

pub struct HandshakeManager {
    authenticated_users: HashMap<SocketAddr, UserKey>,
    been_handshaked_users: HashMap<SocketAddr, UserKey>,

    connection_hash_key: hmac::Key,
    address_to_timestamp_map: HashMap<SocketAddr, Timestamp>,
    timestamp_digest_map: CacheMap<Timestamp, Vec<u8>>,
}

impl HandshakeManager {
    pub fn new() -> Self {
        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        Self {
            authenticated_users: HashMap::new(),
            been_handshaked_users: HashMap::new(),

            connection_hash_key,
            address_to_timestamp_map: HashMap::new(),
            timestamp_digest_map: CacheMap::with_capacity(64),
        }
    }

    pub(crate) fn authenticate_user(&mut self, address: &SocketAddr, user_key: &UserKey) {
        self.authenticated_users.insert(*address, *user_key);
    }

    pub fn maintain_handshake<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        address: &SocketAddr,
        header: &StandardHeader,
        reader: &mut BitReader,
        io: &mut Io,
        user_connections: &mut HashMap<SocketAddr, Connection<E>>,
        time_manager: &mut TimeManager,
    ) -> Result<HandshakeAction, SerdeErr> {
        // Handshake stuff
        match header.packet_type {
            PacketType::ClientChallengeRequest => {
                if let Ok(writer) = self.recv_challenge_request(reader) {
                    if io.send_packet(&address, writer.to_packet()).is_err() {
                        // TODO: pass this on and handle above
                        warn!(
                            "Server Error: Cannot send challenge response packet to {}",
                            &address
                        );
                    }
                }
                return Ok(HandshakeAction::ContinueReadingPacket);
            }
            PacketType::ClientValidateRequest => {
                if self.recv_validate_request(
                    address,
                    reader,
                ) {
                    if self.been_handshaked_users.contains_key(address) {
                        // send validate response
                        let writer = self.write_validate_response();
                        if io.send_packet(address, writer.to_packet()).is_err() {
                            // TODO: pass this on and handle above
                            warn!("Server Error: Cannot send validate success response packet to {}", &address);
                        };
                    } else {
                        info!("checking authenticated users for {}", address);
                        if let Some(user_key) = self.authenticated_users.get(address) {
                            let user_key = *user_key;
                            let address = *address;
                            self.user_finish_handshake(io, &address, &user_key);
                        } else {
                            warn!("Server Error: Cannot find user by address {}", address);
                        }
                    }
                } else {
                    // do nothing
                }
                return Ok(HandshakeAction::ContinueReadingPacket);
            }
            PacketType::ClientConnectRequest => {
                if user_connections.contains_key(address) {
                    // send connect response
                    let writer = self.write_connect_response();
                    if io.send_packet(address, writer.to_packet()).is_err() {
                        // TODO: pass this on and handle above
                        warn!(
                            "Server Error: Cannot send connect success response packet to {}",
                            address
                        );
                    };
                    return Ok(HandshakeAction::ContinueReadingPacket);
                } else {
                    let user_key = *self
                        .been_handshaked_users
                        .get(address)
                        .expect("should be a user by now, from validation step");

                    // send connect response
                    let writer = self.write_connect_response();
                    if io
                        .send_packet(address, writer.to_packet())
                        .is_err()
                    {
                        // TODO: pass this on and handle above
                        warn!(
                            "Server Error: Cannot send connect response packet to {}",
                            address
                        );
                    }

                    return Ok(HandshakeAction::ContinueReadingPacketAndFinalizeConnection(user_key));
                }
            }
            PacketType::Ping => {
                let response = time_manager.process_ping(reader).unwrap();
                // send packet
                if io.send_packet(address, response.to_packet()).is_err() {
                    // TODO: pass this on and handle above
                    warn!("Server Error: Cannot send pong packet to {}", address);
                };
                if let Some(connection) = user_connections.get_mut(address) {
                    connection.base.mark_sent();
                }
                return Ok(HandshakeAction::ContinueReadingPacket);
            }
            _ => {
                return Ok(HandshakeAction::AbortPacket);
            }
        }
    }

    // Step 1 of Handshake
    fn recv_challenge_request(
        &mut self,
        reader: &mut BitReader,
    ) -> Result<BitWriter, SerdeErr> {
        let timestamp = Timestamp::de(reader)?;

        Ok(self.write_challenge_response(&timestamp))
    }

    // Step 2 of Handshake
    fn write_challenge_response(&mut self, timestamp: &Timestamp) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::ServerChallengeResponse, 0, 0, 0).ser(&mut writer);
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
    fn recv_validate_request(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
    ) -> bool {
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
        StandardHeader::new(PacketType::ServerValidateResponse, 0, 0, 0).ser(&mut writer);
        writer
    }

    // Step 5 of Handshake
    fn write_connect_response(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::ServerConnectResponse, 0, 0, 0).ser(&mut writer);
        writer
    }

    pub fn verify_disconnect_request<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        connection: &Connection<E>,
        reader: &mut BitReader,
    ) -> bool {
        // Verify that timestamp hash has been written by this
        // server instance
        if let Some(new_timestamp) = self.timestamp_validate(reader) {
            if let Some(old_timestamp) = self.address_to_timestamp_map.get(&connection.address) {
                if *old_timestamp == new_timestamp {
                    return true;
                }
            }
        }

        false
    }

    // pub fn write_reject_response(&self) -> BitWriter {
    //     let mut writer = BitWriter::new();
    //     StandardHeader::new(PacketType::ServerRejectResponse, 0, 0, 0).ser(&mut writer);
    //     writer
    // }

    pub fn delete_user(&mut self, address: &SocketAddr) {
        self.authenticated_users.remove(address);
        self.been_handshaked_users.remove(address);
        self.address_to_timestamp_map.remove(address);
    }

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

    fn user_finish_handshake(
        &mut self,
        io: &mut Io,
        addr: &SocketAddr,
        user_key: &UserKey
    ) {

        // send validate response
        let writer = self.write_validate_response();
        if io
            .send_packet(addr, writer.to_packet())
            .is_err()
        {
            // TODO: pass this on and handle above
            warn!(
                "Server Error: Cannot send validate response packet to {}",
                addr
            );
        }

        self.been_handshaked_users.insert(*addr, *user_key);
    }
}
