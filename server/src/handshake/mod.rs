use crate::UserKey;
cfg_if! {
    if #[cfg(feature = "advanced_handshake")] {
        mod advanced_handshaker;
        pub use advanced_handshaker::HandshakeManager;
    } else {
        mod simple_handshaker;
        pub use simple_handshaker::HandshakeManager;
    }
}

pub struct HandshakeError;

pub trait Handshaker: Send + Sync {
    fn example(&self) -> Result<(), HandshakeError>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeAction {
    ContinueReadingPacketAndFinalizeConnection(UserKey),
    ContinueReadingPacket,
    AbortPacket,
}

