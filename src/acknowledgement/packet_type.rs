
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum PacketType {
    Data = 1,
    Heartbeat = 2,
    ServerHandshake = 3,
    ClientHandshake = 4,
    Unknown = 255
}

impl From<u8> for PacketType {
    fn from(orig: u8) -> Self {
        match orig {
            1 => return PacketType::Data,
            2 => return PacketType::Heartbeat,
            3 => return PacketType::ServerHandshake,
            4 => return PacketType::ClientHandshake,
            _ => return PacketType::Unknown,
        };
    }
}