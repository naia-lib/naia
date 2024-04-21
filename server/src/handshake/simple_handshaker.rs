
use std::{collections::HashMap, net::SocketAddr};

use log::warn;

use naia_shared::{
    BitReader, BitWriter, PacketType, Serde,
    SerdeErr, StandardHeader, handshake::HandshakeHeader,
};

use crate::{handshake::{HandshakeAction, Handshaker}, UserKey};

pub struct HandshakeManager {
    authenticated_users: HashMap<SocketAddr, UserKey>,
}

impl Handshaker for HandshakeManager {
    fn authenticate_user(&mut self, address: &SocketAddr, user_key: &UserKey) {
        self.authenticated_users.insert(*address, *user_key);
    }

    fn delete_user(&mut self, address: &SocketAddr) {
        self.authenticated_users.remove(address);
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
            HandshakeHeader::ClientConnectRequest => {

                // send connect response
                let writer = self.write_connect_response();
                let packet = writer.to_packet();

                if has_connection {
                    return Ok(HandshakeAction::SendPacket(packet));
                } else {
                    let Some(user_key) = self.authenticated_users.get(address) else {
                        warn!("Server Error: User not authenticated for: {:?}", address);
                        return Ok(HandshakeAction::None);
                    };

                    return Ok(HandshakeAction::FinalizeConnection(*user_key, packet));
                }
            }
            HandshakeHeader::Disconnect => {
                if self.verify_disconnect_request(address, reader) {
                    let user_key = *self
                        .authenticated_users
                        .get(address)
                        .expect("should be a user by now");
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
            authenticated_users: HashMap::new(),
        }
    }

    // Step 5 of Handshake
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