/// Why a connection was terminated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DisconnectReason {
    /// The client sent a graceful disconnect packet.
    ClientDisconnected,
    /// The remote stopped responding within the configured timeout window.
    TimedOut,
    /// The server application explicitly kicked the client.
    Kicked,
    /// The client failed to complete authentication within the auth timeout window.
    AuthTimeout,
}

/// Sequential 16-bit index assigned to each outgoing packet for acknowledgement tracking.
pub type PacketIndex = u16;
/// Server-side tick counter, wrapping at `u16::MAX`.
pub type Tick = u16;
/// Per-channel sequence number for reliable message ordering and deduplication.
pub type MessageIndex = u16;
/// Compact 8-bit message index used by tick-buffered channels.
pub type ShortMessageIndex = u8;

/// Whether a given endpoint is a server or a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostType {
    /// This endpoint is the authoritative server.
    Server,
    /// This endpoint is a connecting client.
    Client,
}

impl HostType {
    /// Returns the opposite host type.
    pub fn invert(self) -> Self {
        match self {
            HostType::Server => HostType::Client,
            HostType::Client => HostType::Server,
        }
    }
}
