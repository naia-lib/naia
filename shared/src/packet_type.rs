use super::standard_header::StandardHeader;

/// An enum representing the different types of packets that can be
/// sent/received
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum PacketType {
    /// A packet containing Event/Entity data
    Data = 1,
    /// A packet sent to maintain the connection by preventing a timeout
    Heartbeat = 2,
    /// An initial handshake message sent by the Client to the Server
    ClientChallengeRequest = 3,
    /// The Server's response to the Client's initial handshake message
    ServerChallengeResponse = 4,
    /// The final handshake message sent by the Client
    ClientConnectRequest = 5,
    /// The final handshake message sent by the Server, indicating that the
    /// connection has been established
    ServerConnectResponse = 6,
    /// An unknown packet type
    Unknown = 255,
}

impl From<u8> for PacketType {
    fn from(orig: u8) -> Self {
        match orig {
            1 => return PacketType::Data,
            2 => return PacketType::Heartbeat,
            3 => return PacketType::ClientChallengeRequest,
            4 => return PacketType::ServerChallengeResponse,
            5 => return PacketType::ClientConnectRequest,
            6 => return PacketType::ServerConnectResponse,
            _ => return PacketType::Unknown,
        };
    }
}

impl PacketType {
    pub fn get_from_packet(payload: &[u8]) -> PacketType {
        StandardHeader::get_packet_type(payload)
    }
}
