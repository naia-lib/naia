mod handshake_time_manager;

use naia_shared::{handshake::RejectReason, BitReader, BitWriter, IdentityToken, OutgoingPacket};

use crate::connection::time_manager::TimeManager;

mod handshaker;
pub use handshaker::HandshakeManager;

pub enum HandshakeResult {
    Connected(Box<TimeManager>),
    Rejected(RejectReason),
}

pub trait Handshaker: Send + Sync {
    fn set_identity_token(&mut self, identity_token: IdentityToken);
    // fn is_connected(&self) -> bool;
    fn send(&mut self) -> Option<OutgoingPacket>;
    fn recv(&mut self, reader: &mut BitReader) -> Option<HandshakeResult>;
    fn write_disconnect(&self) -> BitWriter;
}
