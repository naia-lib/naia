use naia_serde::{BitReader, BitWrite, Serde};

use crate::{packet_type::PacketType, standard_header::StandardHeader};

/// Write a connectionless packet, that is, one that does not rely on
/// information normally retrieved from the connection
pub fn write_connectionless_header<S: BitWrite>(packet_type: PacketType, writer: &mut S) {
    // Add Ack Header onto message!
    StandardHeader::new(packet_type, 0, 0, 0, 0).ser(writer);
}

/// Strip the standard header off of a packet's payload
pub fn read_header(reader: &mut BitReader) {
    StandardHeader::de(reader).unwrap();
}
