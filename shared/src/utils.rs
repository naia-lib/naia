use naia_serde::BitReader;

use crate::{packet_type::PacketType, standard_header::StandardHeader};

/// Write a connectionless packet, that is, one that does not rely on
/// information normally retrieved from the connection
pub fn write_connectionless_header<S>(packet_type: PacketType, writer: &mut S) {
    // Add Ack Header onto message!
    let header = StandardHeader::new(packet_type, 0, 0, 0, 0);
    //Outback
    //header.ser(writer);
}

/// Strip the standard header off of a packet's payload and retrieve the payload
/// bytes
pub fn read_header(reader: &mut BitReader) {
    //Outback
    //return StandardHeader::de(reader).unwrap();
}
