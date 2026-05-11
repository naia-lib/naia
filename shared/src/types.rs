/// Why a connection was terminated. Carried by `DisconnectEvent` on both
/// server and client so game code can respond differently (e.g. show "kicked"
/// vs "connection lost" UI without guesswork).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DisconnectReason {
    /// The remote side sent a clean `HandshakeHeader::Disconnect`.
    ClientDisconnected,
    /// No packets received within `disconnection_timeout_duration`.
    TimedOut,
    /// Server called `disconnect_user()` / `UserMut::disconnect()`.
    Kicked,
    /// Pending-auth window expired before `accept_connection()` was called.
    AuthTimeout,
}

pub type PacketIndex = u16;
pub type Tick = u16;
pub type MessageIndex = u16;
pub type ShortMessageIndex = u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostType {
    Server,
    Client,
}

impl HostType {
    pub fn invert(self) -> Self {
        match self {
            HostType::Server => HostType::Client,
            HostType::Client => HostType::Server,
        }
    }
}
