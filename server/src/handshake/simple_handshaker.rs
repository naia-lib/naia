
use std::{collections::HashMap, net::SocketAddr};

use log::warn;

use naia_shared::{BitReader, BitWriter, PacketType, Serde, SerdeErr, StandardHeader, handshake::HandshakeHeader, IdentityToken};

use crate::{handshake::{HandshakeAction, Handshaker}, UserKey};

pub struct HandshakeManager {
    authenticated_and_identified_users: HashMap<SocketAddr, UserKey>,
    authenticated_unidentified_users: HashMap<IdentityToken, UserKey>,
    identity_token_map: HashMap<UserKey, IdentityToken>,
}

impl Handshaker for HandshakeManager {
    fn authenticate_user(&mut self, identity_token: &IdentityToken, user_key: &UserKey) {
        self.authenticated_unidentified_users.insert(identity_token.clone(), *user_key);
        self.identity_token_map.insert(*user_key, identity_token.clone());
    }

    fn delete_user(&mut self, user_key: &UserKey, address: &SocketAddr) {
        if let Some(identity_token) = self.identity_token_map.remove(user_key) {
            self.authenticated_unidentified_users.remove(&identity_token);
        }
        self.authenticated_and_identified_users.remove(address);
    }

    fn maintain_handshake(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
        has_connection: bool
    ) -> Result<HandshakeAction, SerdeErr> {
        let handshake_header = HandshakeHeader::de(reader)?;

        // Handshake stuff
        match handshake_header {
            HandshakeHeader::ClientIdentifyRequest => {
                if let Ok(identity_token) = self.recv_identify_request(reader) {

                    if let Some(user_key) = self.authenticated_unidentified_users.remove(&identity_token) {

                        // remove identity token from map
                        if self.identity_token_map.remove(&user_key).is_none() {
                            panic!("Server Error: Identity Token not found for user_key: {:?}. Shouldn't be possible.", user_key);
                        }

                        // User is authenticated
                        self.authenticated_and_identified_users.insert(*address, user_key);
                    } else {
                        warn!("Server Error: User not authenticated for: {:?}, with token: {}", address, identity_token);
                        return Ok(HandshakeAction::None);
                    }

                    let identify_response = Self::write_identity_response().to_packet();

                    return Ok(HandshakeAction::SendPacket(identify_response));
                } else {
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
                    let Some(user_key) = self.authenticated_and_identified_users.get(address) else {
                        warn!("Server Error: User not authenticated for: {:?}", address);
                        return Ok(HandshakeAction::None);
                    };

                    return Ok(HandshakeAction::FinalizeConnection(*user_key, packet));
                }
            }
            HandshakeHeader::Disconnect => {
                if self.verify_disconnect_request(address, reader) {
                    let user_key = *self
                        .authenticated_and_identified_users
                        .get(address)
                        .expect("Server Error: User not authenticated for disconnect request. Shouldn't be possible.");
                    return Ok(HandshakeAction::DisconnectUser(user_key));
                } else {
                    return Ok(HandshakeAction::None);
                }
            }
            _ => {
                warn!("Server Error: Unexpected handshake header: {:?} from {}", handshake_header, address);
                return Ok(HandshakeAction::None);
            }
        }
    }
}

impl HandshakeManager {
    pub fn new() -> Self {
        Self {
            authenticated_and_identified_users: HashMap::new(),
            authenticated_unidentified_users: HashMap::new(),
            identity_token_map: HashMap::new(),
        }
    }

    // Step 1 of Handshake
    fn recv_identify_request(
        &mut self,
        reader: &mut BitReader,
    ) -> Result<IdentityToken, SerdeErr> {
        IdentityToken::de(reader)
    }

    // Step 2 of Handshake
    fn write_identity_response() -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerIdentifyResponse.ser(&mut writer);

        writer
    }

    // Step 3 of Handshake
    fn write_connect_response(&self) -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerConnectResponse.ser(&mut writer);
        writer
    }

    fn verify_disconnect_request(
        &mut self,
        _address: &SocketAddr,
        _reader: &mut BitReader,
    ) -> bool {
        // To verify that timestamp hash has been written by this
        // server instance

        todo!()
    }

    // fn write_reject_response(&self) -> BitWriter {
    //     let mut writer = BitWriter::new();
    //     StandardHeader::new(PacketType::ServerRejectResponse, 0, 0, 0).ser(&mut writer);
    //     writer
    // }
}