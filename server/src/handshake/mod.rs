use std::net::SocketAddr;

use naia_shared::{BitReader, DisconnectReason, IdentityToken, OutgoingPacket, SerdeErr};

use crate::UserKey;

#[cfg(feature = "transport_udp")]
mod cache_map;

mod handshaker;
pub use handshaker::HandshakeManager;

pub trait Handshaker: Send + Sync {
    fn authenticate_user(
        &mut self,
        identity_token: &IdentityToken,
        user_key: UserKey,
        address: &SocketAddr,
    );

    // address is optional because user may not have been identified yet
    fn delete_user(&mut self, user_key: UserKey, address_opt: Option<SocketAddr>);

    fn maintain_handshake(
        &mut self,
        address: &SocketAddr,
        reader: &mut BitReader,
        has_connection: bool,
    ) -> Result<HandshakeAction, SerdeErr>;

    fn reset(&mut self);

    /// Take the recorded client-ping count for a handshaking address,
    /// removing it. Defaults to `None` (untracked); `HandshakeManager`
    /// overrides this with its per-address counts.
    fn take_handshake_attempts(&mut self, _address: &SocketAddr) -> Option<u32> {
        None
    }

    /// Write a disconnect packet to send to a client.
    ///
    /// `payload`, when present, is an already-serialized message saying why in
    /// the application's own terms (naia-lib/naia#10).
    fn write_disconnect(&self, reason: DisconnectReason, payload: Option<&[u8]>) -> OutgoingPacket;
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
    /// Reject the identify and delete the never-connected user, so a failed
    /// handshake leaves no half-connected state ([connection-24]). Only
    /// constructed when `transport_udp` is disabled.
    #[cfg_attr(feature = "transport_udp", allow(dead_code))]
    RejectUser(UserKey, OutgoingPacket),
}
