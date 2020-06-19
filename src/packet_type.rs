
use super::standard_header::StandardHeader;

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum PacketType {
    Data = 1,
    Heartbeat = 2,
    ClientChallengeRequest = 3,
    ServerChallengeResponse = 4,
    ClientConnectRequest = 5,
    ServerConnectResponse = 6,
    Unknown = 255
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