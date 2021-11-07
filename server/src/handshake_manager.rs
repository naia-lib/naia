
use std::{
    collections::HashMap,
    hash::Hash,
    net::SocketAddr,
    marker::PhantomData,
};

use byteorder::{BigEndian, WriteBytesExt};
use ring::{hmac, rand};

use naia_server_socket::Packet;

pub use naia_shared::{
    wrapping_diff, Connection, ConnectionConfig, Instant, KeyGenerator, LocalComponentKey,
    ManagerType, Manifest, PacketReader, PacketType, PropertyMutate, PropertyMutator,
    ProtocolKindType, ProtocolType, Replicate, ReplicateSafe, SharedConfig, StandardHeader, Timer,
    Timestamp, WorldMutType, WorldRefType,
};

use super::{
    client_connection::ClientConnection,
    world_record::WorldRecord,
    io::Io,
};

pub enum HandshakeResult<P: ProtocolType> {
    None,
    DisconnectUser,
    AuthUser(P),
    ConnectUser,
}

pub struct HandshakeManager<P: ProtocolType> {
    connection_hash_key: hmac::Key,
    require_auth: bool,
    address_to_timestamp_map: HashMap<SocketAddr, Timestamp>,
    phantom: PhantomData<P>,
}

impl<P: ProtocolType> HandshakeManager<P> {
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

    pub fn receive_challenge_request(&mut self, io: &mut Io, server_tick: u16, address: &SocketAddr, incoming_bytes: &Box<[u8]>) {
        let mut reader = PacketReader::new(incoming_bytes);
        let timestamp = Timestamp::read(&mut reader);

        let mut timestamp_bytes = Vec::new();
        timestamp.write(&mut timestamp_bytes);
        let timestamp_hash: hmac::Tag =
            hmac::sign(&self.connection_hash_key, &timestamp_bytes);

        let mut outgoing_bytes = Vec::new();
        // write current tick
        outgoing_bytes
            .write_u16::<BigEndian>(server_tick)
            .unwrap();

        //write timestamp
        outgoing_bytes.append(&mut timestamp_bytes);

        //write timestamp digest
        let hash_bytes: &[u8] = timestamp_hash.as_ref();
        for hash_byte in hash_bytes {
            outgoing_bytes.push(*hash_byte);
        }

        // Send connectionless //
        let outgoing_packet = Packet::new(*address, outgoing_bytes);
        let new_payload = naia_shared::utils::write_connectionless_payload(
            PacketType::ServerChallengeResponse,
            outgoing_packet.payload(),
        );
        io.send_packet(Packet::new_raw(outgoing_packet.address(), new_payload));
        /////////////////////////
    }

    pub fn receive_new_connect_request(&mut self, manifest: &Manifest<P>, incoming_bytes: &Box<[u8]>) -> HandshakeResult<P> {

        let mut reader = PacketReader::new(incoming_bytes);
        let timestamp = Timestamp::read(&mut reader);

        // Verify that timestamp hash has been written by this
        // server instance
        let mut timestamp_bytes: Vec<u8> = Vec::new();
        timestamp.write(&mut timestamp_bytes);
        let mut digest_bytes: Vec<u8> = Vec::new();
        for _ in 0..32 {
            digest_bytes.push(reader.read_u8());
        }
        let validation_result = hmac::verify(
            &self.connection_hash_key,
            &timestamp_bytes,
            &digest_bytes,
        );
        if validation_result.is_err() {
            return HandshakeResult::None;
        }

        // Timestamp hash is validated, now start configured auth process

        let has_auth = reader.read_u8() == 1;

        if has_auth != self.require_auth {
            return HandshakeResult::None;
        }

        if has_auth {
            let auth_kind = P::Kind::from_u16(reader.read_u16());
            let auth_message =
                manifest.create_replica(auth_kind, &mut reader, 0);
            return HandshakeResult::AuthUser(auth_message);
        } else {
            return HandshakeResult::ConnectUser;
        }
    }

    pub fn receive_old_connect_request<E: Copy + Eq + Hash>(&mut self,
                                       io: &mut Io,
                                       world_record: &WorldRecord<E, P::Kind>,
                                       connection: &mut ClientConnection<P, E>,
                                       incoming_header: &StandardHeader,
                                       incoming_payload: &Box<[u8]>) -> HandshakeResult<P> {

        // At this point, we have already sent the ServerConnectResponse
        // message, but we continue to send the message till the Client
        // stops sending the ClientConnectRequest

        let mut reader = PacketReader::new(incoming_payload);
        let new_timestamp = Timestamp::read(&mut reader);

        if let Some(prev_timestamp) = self.address_to_timestamp_map.get(&connection.get_address()) {
            if *prev_timestamp == new_timestamp {

                connection
                    .process_incoming_header(world_record, &incoming_header);

                // send connect accept message //
                let outgoing_packet = connection.process_outgoing_header(
                    None,
                    0,
                    PacketType::ServerConnectResponse,
                    &[],
                );
                io.send_packet(Packet::new_raw(
                    connection.get_address(),
                    outgoing_packet,
                ));
                connection.mark_sent();
                /////////////////////////////////
            } else {
                return HandshakeResult::DisconnectUser;
            }
        }

        return HandshakeResult::None;
    }

    pub fn delete_user(&mut self, address: &SocketAddr) {
        self.address_to_timestamp_map.remove(address);
    }
}