use std::{collections::HashMap, hash::Hash, marker::PhantomData, net::SocketAddr};

use ring::{hmac, rand};

use naia_server_socket::Packet;

pub use naia_shared::{
    wrapping_diff, BaseConnection, ConnectionConfig, Instant, KeyGenerator, LocalComponentKey,
    ManagerType, Manifest, PacketType, PropertyMutate, PropertyMutator,
    ProtocolKindType, Protocolize, Replicate, ReplicateSafe, SharedConfig, StandardHeader, Timer,
    Timestamp, WorldMutType, WorldRefType, serde::{BitReader, Serde}
};
use naia_shared::serde::BitWriter;

use super::{connection::Connection, io::Io, world_record::WorldRecord};

pub enum HandshakeResult<P: Protocolize> {
    Invalid,
    AuthUser(P),
    ConnectUser,
}

pub struct HandshakeManager<P: Protocolize> {
    connection_hash_key: hmac::Key,
    require_auth: bool,
    address_to_timestamp_map: HashMap<SocketAddr, Timestamp>,
    phantom: PhantomData<P>,
}

impl<P: Protocolize> HandshakeManager<P> {
    pub fn new(require_auth: bool) -> Self {
        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        Self {
            connection_hash_key,
            require_auth,
            address_to_timestamp_map: HashMap::new(),
            phantom: PhantomData,
        }
    }

    pub fn receive_challenge_request(
        &mut self,
        io: &mut Io,
        address: &SocketAddr,
        reader: &mut BitReader
    ) {
        let timestamp = u64::de(reader).unwrap();

        let mut writer = BitWriter::new();
        let mut timestamp_writer = BitWriter::new();
        StandardHeader::new(PacketType::ServerChallengeResponse, 0, 0, 0, 0)
            .ser(&mut writer);
        timestamp.ser(&mut writer);
        timestamp.ser(&mut timestamp_writer);
        let (timestamp_length, timestamp_bytes) = timestamp_writer.flush();
        let timestamp_tag = hmac::sign(&self.connection_hash_key, &timestamp_bytes[..timestamp_length]);
        let timestamp_hash: &[u8] = timestamp_tag.as_ref();

        //write timestamp digest
        timestamp_hash.ser(&mut writer);

        io.send_writer(address, &mut writer);
    }

    fn timestamp_validate(&self, reader: &mut BitReader) -> Option<Timestamp> {
        let timestamp = u64::de(reader).unwrap();
        let mut digest_bytes: Vec<u8> = Vec::new();
        for _ in 0..32 {
            digest_bytes.push(u8::de(reader).unwrap());
        }

        // Verify that timestamp hash has been written by this
        // server instance
        let mut timestamp_writer = BitWriter::new();
        timestamp.ser(&mut timestamp_writer);
        let (timestamp_length, timestamp_bytes) = timestamp_writer.flush();

        let validation_result =
            hmac::verify(&self.connection_hash_key, &timestamp_bytes[..timestamp_length], &digest_bytes);
        if validation_result.is_err() {
            return None;
        } else {
            return Some(Timestamp::from_u64(&timestamp));
        }
    }

    pub fn receive_new_connect_request(
        &mut self,
        manifest: &Manifest<P>,
        address: &SocketAddr,
        reader: &mut BitReader,
    ) -> HandshakeResult<P> {
        // Verify that timestamp hash has been written by this
        // server instance
        if let Some(timestamp) = self.timestamp_validate(reader) {
            // Timestamp hash is validated, now start configured auth process
            let has_auth = u8::de(reader).unwrap() == 1;

            if has_auth != self.require_auth {
                return HandshakeResult::Invalid;
            }

            self.address_to_timestamp_map.insert(*address, timestamp);

            if has_auth {
                let auth_kind = P::Kind::de(reader).unwrap();
                let auth_message = manifest.create_replica(auth_kind, reader);
                return HandshakeResult::AuthUser(auth_message);
            } else {
                return HandshakeResult::ConnectUser;
            }
        } else {
            return HandshakeResult::Invalid;
        }
    }

    pub fn receive_old_connect_request<E: Copy + Eq + Hash>(
        &mut self,
        io: &mut Io,
        world_record: &WorldRecord<E, P::Kind>,
        connection: &mut Connection<P, E>,
        incoming_header: &StandardHeader,
        reader: &mut BitReader,
    ) {
        // At this point, we have already sent the ServerConnectResponse
        // message, but we continue to send the message till the Client
        // stops sending the ClientConnectRequest

        // Verify that timestamp hash has been written by this
        // server instance
        if let Some(new_timestamp) = self.timestamp_validate(reader) {
            if let Some(old_timestamp) = self.address_to_timestamp_map.get(&connection.base.address)
            {
                if *old_timestamp == new_timestamp {
                    connection.process_incoming_header(world_record, &incoming_header);

                    // send connect accept response
                    let mut writer = BitWriter::new();
                    connection
                            .base
                            .write_outgoing_header(0, PacketType::ServerConnectResponse, &mut writer);
                    io.send_writer(&connection.base.address, &mut writer);
                    connection.base.mark_sent();
                }
            }
        }
    }

    pub fn verify_disconnect_request<E: Copy + Eq + Hash>(
        &mut self,
        connection: &mut Connection<P, E>,
        reader: &mut BitReader,
    ) -> bool {

        // Verify that timestamp hash has been written by this
        // server instance
        if let Some(new_timestamp) = self.timestamp_validate(reader) {
            if let Some(old_timestamp) = self.address_to_timestamp_map.get(&connection.base.address)
            {
                if *old_timestamp == new_timestamp {
                    return true;
                }
            }
        }

        return false;
    }

    pub fn delete_user(&mut self, address: &SocketAddr) {
        self.address_to_timestamp_map.remove(address);
    }
}
