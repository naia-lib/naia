// An enum representing the different types of packets that can be
// sent/received

use naia_serde::{
    wire_schema_custom_leaf, BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedInteger,
    WireSchema, WireSchemaContext,
};

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

// Schema descriptors //

// Custom leaf: the biased-bit grammar above is bespoke (a `Data` fast-path
// bit plus a 2-bit index for the rest), so the descriptor pins it by its
// curated identifier rather than pretending it is a standard enum. Any
// change to `ser`/`de` must change the identifier.
impl WireSchema for PacketType {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        wire_schema_custom_leaf(out, "PacketType");
    }
}

#[cfg(test)]
mod schema_tests {
    use naia_serde::{WireSchema, SCHEMA_TAG_CUSTOM_LEAF, WIRE_SCHEMA_DOMAIN};

    use super::PacketType;

    /// The leaf identifier is curated and stable: it names the bespoke
    /// grammar, and it must not collide with any structural tag.
    #[test]
    fn packet_type_describes_as_a_named_custom_leaf() {
        let bytes = PacketType::wire_schema_bytes();
        assert_eq!(&bytes[..WIRE_SCHEMA_DOMAIN.len()], WIRE_SCHEMA_DOMAIN);
        assert_eq!(bytes[WIRE_SCHEMA_DOMAIN.len()], SCHEMA_TAG_CUSTOM_LEAF);
        assert_eq!(
            &bytes[WIRE_SCHEMA_DOMAIN.len() + 5..],
            b"PacketType",
            "id-len u32 LE (10) then the curated identifier",
        );
    }
}
