// An enum representing the different types of packets that can be
// sent/received

use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedInteger};

/// Wire-level packet classification encoded in every packet header.
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub enum PacketType {
    /// Contains message, entity, and component replication data.
    Data,
    /// Keep-alive packet sent to prevent connection timeout.
    Heartbeat,
    /// Client-initiated handshake packet.
    Handshake,
    /// RTT probe — must be replied to with a `Pong`.
    Ping,
    /// RTT response to a `Ping`.
    Pong,
}

// Most packets should be Data, so lets compress this a bit more.
// Could do this with another enum, but code would get messy.
impl Serde for PacketType {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let is_data = *self == PacketType::Data;
        is_data.ser(writer);

        if is_data {
            return;
        }

        let index = match self {
            PacketType::Data => panic!("shouldn't happen, caught above"),
            PacketType::Heartbeat => 0,
            PacketType::Handshake => 1,
            PacketType::Ping => 2,
            PacketType::Pong => 3,
        };

        UnsignedInteger::<2>::new(index).ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let is_data = bool::de(reader)?;
        if is_data {
            return Ok(PacketType::Data);
        }

        match UnsignedInteger::<2>::de(reader)?.get() {
            0 => Ok(PacketType::Heartbeat),
            1 => Ok(PacketType::Handshake),
            2 => Ok(PacketType::Ping),
            3 => Ok(PacketType::Pong),
            _ => panic!("shouldn't happen, caught above"),
        }
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;

        let is_data = *self == PacketType::Data;
        output += is_data.bit_length();

        if is_data {
            return output;
        }

        output += <UnsignedInteger<4> as ConstBitLength>::const_bit_length();

        output
    }
}
