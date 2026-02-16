use std::net::SocketAddr;

use naia_shared::{BitReader, IdentityToken, OutgoingPacket, SerdeErr};

use crate::UserKey;

cfg_if! {
    if #[cfg(feature = "transport_udp")] {
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

    // address is optional because user may not have been identified yet
    fn delete_user(&mut self, user_key: &UserKey, address_opt: Option<SocketAddr>);

    fn maintain_handshake(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
        has_connection: bool,
    ) -> Result<HandshakeAction, SerdeErr>;

    fn reset(&mut self);

    /// Write a disconnect packet to send to a client
    fn write_disconnect(&self) -> OutgoingPacket;
}

pub enum HandshakeAction {
    None,
    FinalizeConnection(UserKey, OutgoingPacket),
    SendPacket(OutgoingPacket),
    /// Used by the simple (non-UDP) handshaker to forward unrecognized packets.
    /// Only constructed when `transport_udp` is disabled.
    #[cfg_attr(feature = "transport_udp", allow(dead_code))]
    ForwardPacket,
    /// Disconnect the user (for verified disconnect requests)
    DisconnectUser(UserKey),
}
