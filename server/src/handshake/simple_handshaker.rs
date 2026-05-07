use std::{collections::HashMap, net::SocketAddr};

use log::warn;

use naia_shared::{
    handshake::{HandshakeHeader, RejectReason},
    BitReader, BitWriter, IdentityToken, PacketType, ProtocolId, Serde, SerdeErr, StandardHeader,
};

use crate::{
    handshake::{HandshakeAction, Handshaker},
    UserKey,
};

pub struct HandshakeManager {
    protocol_id: ProtocolId,
    authenticated_and_identified_users: HashMap<SocketAddr, UserKey>,
    authenticated_unidentified_users: HashMap<IdentityToken, UserKey>,
    identity_token_map: HashMap<UserKey, IdentityToken>,
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
            HandshakeHeader::ClientIdentifyRequest(protocol_id) => {
                if protocol_id != self.protocol_id {
                    warn!(
                        "Server: Protocol Mismatch! Client: {}, Server: {}",
                        protocol_id, self.protocol_id
                    );
                    let reject_response =
                        Self::write_reject_response(RejectReason::ProtocolMismatch).to_packet();
                    return Ok(HandshakeAction::SendPacket(reject_response));
                }
                if has_connection {
                    let identify_response = Self::write_identity_response().to_packet();
                    Ok(HandshakeAction::SendPacket(identify_response))
                } else {
                    let Ok(id_token) = self.recv_identify_request(reader) else {
                        return Ok(HandshakeAction::None);
                    };
                    let Some(user_key) = self.authenticated_unidentified_users.remove(&id_token)
                    else {
                        let reject_response =
                            Self::write_reject_response(RejectReason::Auth).to_packet();
                        return Ok(HandshakeAction::SendPacket(reject_response));
                    };
                    // Verify identity token exists (but keep it for disconnect verification)
                    if !self.identity_token_map.contains_key(&user_key) {
                        panic!("Server Error: Identity Token not found for user_key: {:?}. Shouldn't be possible.", user_key);
                    }

                    // User is authenticated
                    self.authenticated_and_identified_users
                        .insert(*address, user_key);

                    // send identify response
                    let identify_response = Self::write_identity_response().to_packet();
                    Ok(HandshakeAction::FinalizeConnection(
                        user_key,
                        identify_response,
                    ))
                }
            }
            HandshakeHeader::ClientConnectRequest => {
                Ok(HandshakeAction::ForwardPacket)
            }
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
                    "Server Error: Unexpected handshake header: {:?} from {}",
                    handshake_header, address
                );
                Ok(HandshakeAction::None)
            }
        }
    }

    fn reset(&mut self) {
        self.authenticated_and_identified_users.clear();
        self.authenticated_unidentified_users.clear();
        self.identity_token_map.clear();
    }

    fn write_disconnect(&self) -> naia_shared::OutgoingPacket {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::Disconnect.ser(&mut writer);
        writer.to_packet()
    }
}

impl HandshakeManager {
    pub fn new(protocol_id: ProtocolId) -> Self {
        Self {
            protocol_id,
            authenticated_and_identified_users: HashMap::new(),
            authenticated_unidentified_users: HashMap::new(),
            identity_token_map: HashMap::new(),
        }
    }

    // Step 1 of Handshake
    fn recv_identify_request(&mut self, reader: &mut BitReader) -> Result<IdentityToken, SerdeErr> {
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
    pub(crate) fn write_connect_response() -> BitWriter {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerConnectResponse.ser(&mut writer);
        writer
    }

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
}
