use std::net::SocketAddr;

use naia_shared::{BitReader, IdentityToken, OutgoingPacket, SerdeErr};

use crate::UserKey;

cfg_if! {
    if #[cfg(feature = "advanced_handshake")] {
        mod cache_map;

        mod advanced_handshaker;
        pub use advanced_handshaker::HandshakeManager;
    } else {
        mod simple_handshaker;
        pub use simple_handshaker::HandshakeManager;
    }
}

pub trait Handshaker: Send + Sync {
    fn authenticate_user(&mut self, identity_token: &IdentityToken, user_key: &UserKey);
    fn delete_user(&mut self, user_key: &UserKey, address: &SocketAddr);
    fn maintain_handshake(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
        has_connection: bool,
    ) -> Result<HandshakeAction, SerdeErr>;
}

pub enum HandshakeAction {
    None,
    FinalizeConnection(UserKey, OutgoingPacket),
    SendPacket(OutgoingPacket),
    DisconnectUser(UserKey),
}

