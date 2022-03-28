use std::{collections::HashMap, hash::Hash, marker::PhantomData, net::SocketAddr};

use ring::{hmac, rand};

use naia_shared::ChannelIndex;
pub use naia_shared::{
    serde::{BitReader, BitWriter, Serde},
    wrapping_diff, BaseConnection, ConnectionConfig, FakeEntityConverter, Instant, KeyGenerator,
    PacketType, PropertyMutate, PropertyMutator, ProtocolKindType, Protocolize, Replicate,
    ReplicateSafe, SharedConfig, StandardHeader, Timer, WorldMutType, WorldRefType,
};

use super::{cache_map::CacheMap, connection::Connection};

pub type Timestamp = u64;

pub enum HandshakeResult<P: Protocolize> {
    Invalid,
    Success(Option<P>),
}

pub struct HandshakeManager<P: Protocolize> {
    connection_hash_key: hmac::Key,
    require_auth: bool,
    address_to_timestamp_map: HashMap<SocketAddr, Timestamp>,
    timestamp_digest_map: CacheMap<Timestamp, Vec<u8>>,
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
            timestamp_digest_map: CacheMap::with_capacity(64),
            phantom: PhantomData,
        }
    }

    // Step 1 of Handshake
    pub fn recv_challenge_request(&mut self, reader: &mut BitReader) -> BitWriter {
        let timestamp = Timestamp::de(reader).unwrap();

        let writer = self.write_challenge_response(&timestamp);
        writer
    }

    // Step 2 of Handshake
    pub fn write_challenge_response(&mut self, timestamp: &Timestamp) -> BitWriter {
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
    pub fn recv_connect_request(&mut self, reader: &mut BitReader) -> HandshakeResult<P> {
        // Verify that timestamp hash has been written by this
        // server instance
        if self.timestamp_validate(reader).is_some() {
            // Timestamp hash is validated, now start configured auth process
            let has_auth = bool::de(reader).unwrap();

            if has_auth != self.require_auth {
                return HandshakeResult::Invalid;
            }

            if has_auth {
                let auth_message = P::build(reader, &FakeEntityConverter);
                return HandshakeResult::Success(Some(auth_message));
            } else {
                return HandshakeResult::Success(None);
            }
        } else {
            return HandshakeResult::Invalid;
        }
    }

    // Step 3 of Handshake
    pub fn write_connect_response(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::ServerConnectResponse, 0, 0, 0).ser(&mut writer);
        writer
    }

    pub fn verify_disconnect_request<E: Copy + Eq + Hash, C: ChannelIndex>(
        &mut self,
        connection: &Connection<P, E, C>,
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

    fn timestamp_validate(&self, reader: &mut BitReader) -> Option<Timestamp> {
        // Read timestamp
        let timestamp = Timestamp::de(reader).unwrap();
        let digest_bytes: Vec<u8> = Vec::<u8>::de(reader).unwrap();

        // Verify that timestamp hash has been written by this
        // server instance
        let validation_result = hmac::verify(
            &self.connection_hash_key,
            &timestamp.to_le_bytes(),
            &digest_bytes,
        );
        if validation_result.is_err() {
            return None;
        } else {
            return Some(timestamp);
        }
    }
}
