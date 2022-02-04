/// An enum representing the different types of packets that can be
/// sent/received
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum PacketType {
    /// A packet containing Message/Entity/Component data
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
    /// A Ping message, used to calculate RTT. Must be responded to with a Pong
    /// message
    Ping = 7,
    /// A Pong message, used to calculate RTT. Must be the response to all Ping
    /// messages
    Pong = 8,
    /// Used to request a graceful Client disconnect from the Server
    Disconnect = 9,
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
            7 => return PacketType::Ping,
            8 => return PacketType::Pong,
            9 => return PacketType::Disconnect,
            _ => return PacketType::Unknown,
        };
    }
}
