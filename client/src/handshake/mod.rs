mod handshake_time_manager;

use naia_shared::{BitReader, BitWriter, IdentityToken, OutgoingPacket};

use crate::connection::time_manager::TimeManager;

cfg_if! {
    if #[cfg(feature = "advanced_handshake")] {
        mod advanced_handshaker;
        pub use advanced_handshaker::HandshakeManager;
    } else {
        mod simple_handshaker;
        pub use simple_handshaker::HandshakeManager;
    }
}

pub enum HandshakeResult {
    Connected(TimeManager),
}

pub trait Handshaker: Send + Sync {
    fn set_identity_token(&mut self, identity_token: IdentityToken);
    fn is_connected(&self) -> bool;
    fn send(&mut self) -> Option<OutgoingPacket>;
    fn recv(&mut self, reader: &mut BitReader) -> Option<HandshakeResult>;
    fn write_disconnect(&self) -> BitWriter;
}